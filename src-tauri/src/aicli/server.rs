//! Embedded localhost HTTP listener for agent hook events.
//!
//! Claude Code delivers events via its native `type:"http"` hook (POST JSON
//! to a URL). Codex's command hook POSTs via `curl`. Both target
//! `http://127.0.0.1:<PORT>/hook`. The listener parses the body into a
//! `HookEvent` and pushes it onto an `mpsc` channel that `listen.rs` drains.
//!
//! Loopback-only (`127.0.0.1`) — never network-exposed. Fail-open: a parse
//! error or unknown source is dropped, never blocks the agent.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use std::thread;

use serde::{Deserialize, Serialize};

use super::model::CliId;

/// Fixed listener port. Written into the managed hook configs so the agents
/// know where to POST. Picked high to avoid clashes; if it's already bound
/// we fall back to an ephemeral port and the hook status UI surfaces that.
pub const HOOK_PORT: u16 = 47823;

/// A single hook event delivered by an agent. `source` disambiguates which
/// agent emitted it (Claude/Codex).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    pub source: CliId,
    pub event: String,
    pub payload: serde_json::Value,
    pub ts_ms: i64,
}

/// Channel shared between the listener thread and the poll loop.
pub struct HookChannel {
    tx: Sender<HookEvent>,
    rx: Mutex<Receiver<HookEvent>>,
}

impl HookChannel {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self { tx, rx: Mutex::new(rx) }
    }

    /// Drain all pending events (non-blocking).
    pub fn drain(&self) -> Vec<HookEvent> {
        let Ok(rx) = self.rx.lock() else { return Vec::new() };
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            out.push(ev);
        }
        out
    }
}

static CHANNEL: std::sync::OnceLock<HookChannel> = std::sync::OnceLock::new();

fn channel() -> &'static HookChannel {
    CHANNEL.get_or_init(HookChannel::new)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Map a raw POST into a `HookEvent`, resolving the source + event name from
/// the agent-specific payloads.
fn classify(source: CliId, body: &[u8]) -> Option<HookEvent> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let event = match source {
        CliId::Claude => v
            .get("hookEventName")
            .and_then(|e| e.as_str())
            .unwrap_or("unknown")
            .to_string(),
        CliId::Codex => v
            .get("hook_event_name")
            .and_then(|e| e.as_str())
            .or_else(|| v.get("event").and_then(|e| e.as_str()))
            .unwrap_or("unknown")
            .to_string(),
        CliId::Opencode => v
            .get("type")
            .and_then(|e| e.as_str())
            .unwrap_or("unknown")
            .to_string(),
    };
    Some(HookEvent {
        source,
        event,
        payload: v,
        ts_ms: now_ms(),
    })
}

/// Handle one request on the `/hook` endpoint. Returns (status_code, body).
fn handle_request(method: &str, path: &str, body: &[u8]) -> (u16, &'static str) {
    if method != "POST" {
        return (405, "method not allowed");
    }
    let source = if path.contains("src=claude") {
        Some(CliId::Claude)
    } else if path.contains("src=codex") {
        Some(CliId::Codex)
    } else {
        // Fall back to a body field the agents may carry.
        classify_source_from_body(body)
    };
    let Some(source) = source else {
        return (400, "unknown source");
    };
    match classify(source, body) {
        Some(ev) => {
            let _ = channel().tx.send(ev);
            (200, "ok")
        }
        None => (400, "bad payload"),
    }
}

fn classify_source_from_body(body: &[u8]) -> Option<CliId> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let s = v.get("source").and_then(|s| s.as_str()).or_else(|| {
        v.get("cli").and_then(|s| s.as_str())
    })?;
    match s {
        "claude" => Some(CliId::Claude),
        "codex" => Some(CliId::Codex),
        "opencode" => Some(CliId::Opencode),
        _ => None,
    }
}

/// Start the listener thread. Binds `127.0.0.1:HOST_PORT` and accepts a single
/// `/hook` route. A tiny hand-rolled TCP server (no framework) keeps RAM and
/// binary size minimal, consistent with the project's footprint goals.
pub fn start() {
    thread::spawn(move || {
        let addr = format!("127.0.0.1:{}", HOOK_PORT);
        let listener = match std::net::TcpListener::bind(&addr) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[aicli:hook] bind {addr} failed: {e}");
                return;
            }
        };
        eprintln!("[aicli:hook] listening on {addr}");
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            thread::spawn(move || handle_stream(stream));
        }
    });
}

fn handle_stream(stream: std::net::TcpStream) {
    use std::io::{BufRead, BufReader, Read, Write};
    let mut reader = BufReader::new(stream.try_clone().ok().unwrap_or(stream));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];

    // Read headers to find Content-Length.
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = rest.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    if reader.read_exact(&mut body).is_err() {
        body.clear();
    }

    // Only handle the /hook route; everything else is a 404.
    let (status, msg) = if path.starts_with("/hook") {
        handle_request(method, path, &body)
    } else {
        (404, "not found")
    };

    let response = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        msg.len(),
        msg
    );
    let _ = reader.get_mut().write_all(response.as_bytes());
}

/// Pull all pending hook events. Called by `listen.rs` on each poll cycle.
pub fn drain_events() -> Vec<HookEvent> {
    channel().drain()
}
