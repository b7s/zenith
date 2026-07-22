//! Tiny localhost HTTP server that receives hook POST events from Claude Code
//! and Codex (via the sidecar) and forwards them to the aggregator.
//!
//! Listens on `127.0.0.1:<port>`. The port is written to
//! `%APPDATA%\zenith\ai-cli-bridge.json` for the sidecar to discover.
//! Endpoints:
//!   POST /ai-cli/event?cli=claude&event=failed  body: JSON hook payload
//!   GET  /ai-cli/health                           → {"healthy": true}

use std::io::{BufRead, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::Emitter;

use super::aggregator::Aggregator;
use super::model::{CliEvent, CliEventType, CliId};

static PORT_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Write the bridge port file so the sidecar can find our endpoint.
fn write_port_file(port: u16) {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("zenith");
    let _ = std::fs::create_dir_all(&base);
    let path = base.join("ai-cli-bridge.json");
    let json = serde_json::json!({ "port": port }).to_string();
    let _ = std::fs::write(&path, &json);
    let _ = PORT_PATH.lock().map(|mut p| *p = Some(path));
}

pub fn bridge_port() -> Option<u16> {
    let raw = std::fs::read_to_string(bridge_path()?).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("port").and_then(|p| p.as_u64()).map(|p| p as u16)
}

fn bridge_path() -> Option<PathBuf> {
    PORT_PATH.lock().ok().and_then(|p| p.clone())
}

/// Spawn the bridge listener on a background thread. Returns the port.
pub fn spawn(aggregator: std::sync::Arc<Mutex<Aggregator>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("ai-cli bridge bind");
    let port = listener.local_addr().unwrap().port();
    write_port_file(port);
    eprintln!("[zenith:ai-cli:bridge] listening on 127.0.0.1:{port} (port file at %APPDATA%/zenith/ai-cli-bridge.json)");

    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let agg = aggregator.clone();
            std::thread::spawn(move || handle_connection(stream, agg));
        }
    });

    port
}

fn handle_connection(mut stream: TcpStream, aggregator: std::sync::Arc<Mutex<Aggregator>>) {
    let _peer = stream.peer_addr().ok();
    let mut reader = std::io::BufReader::new(&mut stream);
    // Read the request line + headers until empty line
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        respond(&stream, 400, "Bad Request");
        return;
    }
    let method = parts[0];
    let path = parts[1];

    // Read headers
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().starts_with("content-length:") {
            content_length = header
                .split(':')
                .nth(1)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0);
        }
    }

    // Read body
    let mut body = Vec::with_capacity(content_length);
    for _ in 0..content_length {
        let mut byte = [0u8];
        if reader.read(&mut byte).is_err() {
            break;
        }
        body.push(byte[0]);
    }
    // Consume trailing newline if any
    let _ = reader.read_line(&mut String::new());

    if method == "GET" && path == "/ai-cli/health" {
        respond_json(&stream, &serde_json::json!({ "healthy": true }));
        return;
    }

    if method == "POST" && path.starts_with("/ai-cli/event") {
        let query: std::collections::HashMap<String, String> =
            url::form_urlencoded::parse(path.split('?').nth(1).unwrap_or("").as_bytes())
                .into_owned()
                .collect();
        let raw_body = String::from_utf8_lossy(&body).to_string();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);

        let cli_str = query
            .get("cli")
            .map(|s| s.as_str())
            .or_else(|| payload.get("cli").and_then(|v| v.as_str()))
            .unwrap_or("");
        let event_str = query
            .get("event")
            .map(|s| s.as_str())
            .or_else(|| payload.get("event").and_then(|v| v.as_str()))
            .unwrap_or("");

        eprintln!("[zenith:ai-cli:bridge] POST path={path} cli={cli_str:?} event={event_str:?} body={raw_body}");
        let Some(cli_id) = CliId::parse(cli_str) else {
            eprintln!("[zenith:ai-cli:bridge] unknown cli={cli_str:?}");
            respond(&stream, 400, "Unknown cli");
            return;
        };

        let event_type = match event_str {
            "started" | "start" | "session-start" => CliEventType::Started,
            "idle" | "stop" => CliEventType::Idle,
            "completed" | "session-end" => CliEventType::Completed,
            "failed" | "stop-failure" | "error" => CliEventType::Failed,
            "waiting" | "confirmation" => CliEventType::Waiting,
            _ => CliEventType::Idle,
        };

        let prompt_label = payload
            .get("prompt_label")
            .or_else(|| payload.get("tool_input").and_then(|t| t.get("command")))
            .and_then(|v| v.as_str())
            .map(|s| {
                let s = s.trim();
                if s.len() > 120 {
                    format!("{}…", &s[..120])
                } else {
                    s.to_string()
                }
            });

        let error_message = if matches!(event_type, CliEventType::Failed) {
            payload
                .get("error_message")
                .or_else(|| payload.get("permissionDecisionReason"))
                .or_else(|| payload.get("tool_input").and_then(|t| t.get("command")))
                .and_then(|v| v.as_str())
                .map(|s| {
                    let s = s.trim();
                    if s.len() > 200 {
                        format!("{}…", &s[..200])
                    } else {
                        s.to_string()
                    }
                })
                .or_else(|| Some("Unknown error".into()))
        } else {
            None
        };

        let event = CliEvent {
            cli_id,
            event_type,
            prompt_label,
            error_message,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        };

        if let Ok(mut agg) = aggregator.lock() {
            agg.ingest(event);
        }
        if let Some(h) = crate::shared::app_handle() {
            let state = super::aggregator().lock().unwrap().aggregate();
            let _ = h.emit(crate::shared::EVENT_AI_CLI_CHANGED, &state);
        }

        respond_json(&stream, &serde_json::json!({ "ok": true }));
        return;
    }

    respond(&stream, 404, "Not Found");
}

fn respond(stream: &TcpStream, status: u16, body: &str) {
    let _ = write_response(stream, status, body);
}

fn respond_json(stream: &TcpStream, value: &serde_json::Value) {
    let body = serde_json::to_string(value).unwrap_or_default();
    let _ = write_response(stream, 200, &body);
}

fn write_response(mut stream: &TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = if status == 200 { "OK" } else { "Error" };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}
