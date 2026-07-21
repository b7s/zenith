//! opencode HTTP/SSE client: discovers the opencode server port and
//! subscribes to the SSE /event stream for live session changes.
//!
//! Falls back to polling /session/status every 2 s if SSE breaks.

use std::io::{BufRead, BufReader};
use std::sync::Mutex;
use std::time::Duration;

use super::aggregator::Aggregator;
use super::model::{CliEvent, CliEventType, CliId};

/// Try to find an opencode server listening on localhost.
/// Checks `~/.cache/opencode/` for port file first, then scans known ports via
/// TCP connect, then falls back to `GetExtendedTcpTable`.
fn discover_opencode_port() -> Option<u16> {
    // 1. Try cache port file (%LOCALAPPDATA%/opencode)
    let cache_dir = std::env::var("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
        .join("opencode");
    let port_file = cache_dir.join("port");
    if let Ok(raw) = std::fs::read_to_string(&port_file) {
        if let Ok(port) = raw.trim().parse::<u16>() {
            if check_health(port) {
                return Some(port);
            }
        }
    }

    // 2. Try common ports
    for port in [4096, 4097, 4098] {
        if check_health(port) {
            return Some(port);
        }
    }

    // 3. Scan via GetExtendedTcpTable
    scan_tcp_table()
}

fn check_health(port: u16) -> bool {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(300))
        .build();
    match agent
        .get(&format!("http://127.0.0.1:{port}/global/health"))
        .call()
    {
        Ok(resp) => resp.into_json::<serde_json::Value>().ok()
            .and_then(|v| v.get("healthy").and_then(|h| h.as_bool()))
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn scan_tcp_table() -> Option<u16> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, TCP_TABLE_OWNER_PID_ALL,
    };
    const AF_INET: u32 = 2;

    let mut buf_size: u32 = 0;
    unsafe {
        let _ = GetExtendedTcpTable(
            None,
            &mut buf_size as *mut u32,
            false,
            AF_INET,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        );
    }
    let mut buf = vec![0u8; buf_size as usize];
    let result = unsafe {
        GetExtendedTcpTable(
            Some(buf.as_mut_ptr() as _),
            &mut buf_size as *mut u32,
            false,
            AF_INET,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if result != 0 {
        return None;
    }

    // Parse the MIB_TCPTABLE_OWNER_PID struct
    let num_entries = u32::from_ne_bytes(buf[..4].try_into().unwrap_or([0u8; 4])) as usize;
    let entry_size: usize = 24; // MIB_TCPROW_OWNER_PID
    for i in 0..num_entries {
        let offset = 4 + i * entry_size;
        if offset + entry_size > buf.len() {
            break;
        }
        let state = u32::from_ne_bytes(buf[offset..offset + 4].try_into().unwrap_or([0u8; 4]));
        let local_addr = u32::from_ne_bytes(buf[offset + 4..offset + 8].try_into().unwrap_or([0u8; 4]));
        let local_port = u16::from_be_bytes(buf[offset + 8..offset + 10].try_into().unwrap_or([0u8; 2]));
        // state == 2 means MIB_TCP_STATE_LISTEN
        if state == 2 && local_addr == 0x0100007f {
            if check_health(local_port) {
                return Some(local_port);
            }
        }
    }
    None
}

/// Spawn the opencode listener thread. Connects to opencode server, subscribes
/// to SSE /event, and feeds events into the aggregator.
pub fn spawn(aggregator: std::sync::Arc<Mutex<Aggregator>>) {
    std::thread::spawn(move || {
        let mut backoff = Duration::from_secs(1);
        loop {
            if let Some(port) = discover_opencode_port() {
                eprintln!("[zenith:ai-cli] opencode server found on :{port}");
                if let Err(e) = sse_loop(port, &aggregator) {
                    eprintln!("[zenith:ai-cli] opencode SSE disconnected ({e}), reconnecting…");
                }
                backoff = Duration::from_secs(1);
            } else {
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(30));
            }
        }
    });
}

fn sse_loop(
    port: u16,
    aggregator: &std::sync::Arc<Mutex<Aggregator>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let agent = ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(300))
        .timeout(Duration::from_secs(300))
        .build();
    let resp = agent
        .get(&format!("http://127.0.0.1:{port}/event"))
        .call()?;
    let reader = BufReader::new(resp.into_reader());
    let mut event_type = String::new();
    let mut data = String::new();

    for line in reader.lines() {
        let line = line?;
        if let Some(field) = line.strip_prefix("event: ") {
            event_type = field.trim().to_string();
            data.clear();
        } else if let Some(field) = line.strip_prefix("data: ") {
            data = field.to_string();
        } else if line.is_empty() {
            if event_type.is_empty() || data.is_empty() {
                continue;
            }
            if let Err(e) = handle_sse_event(&event_type, &data, aggregator) {
                eprintln!("[zenith:ai-cli] sse parse error: {e}");
            }
            event_type.clear();
            data.clear();
        }
    }
    Ok(())
}

fn handle_sse_event(
    event_type: &str,
    data: &str,
    aggregator: &std::sync::Arc<Mutex<Aggregator>>,
) -> Result<(), String> {
    let cli_event = match event_type {
        "session.idle" | "session.updated" => {
            let v: serde_json::Value =
                serde_json::from_str(data).map_err(|e| format!("json: {e}"))?;
            let is_running = v
                .get("status")
                .and_then(|s| s.as_str())
                .map(|s| s == "running")
                .unwrap_or(false);
            if is_running {
                Some(CliEvent {
                    cli_id: CliId::Opencode,
                    event_type: CliEventType::Started,
                    prompt_label: None,
                    error_message: None,
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                })
            } else {
                Some(CliEvent {
                    cli_id: CliId::Opencode,
                    event_type: CliEventType::Idle,
                    prompt_label: None,
                    error_message: None,
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                })
            }
        }
        "session.error" => {
            let v: serde_json::Value =
                serde_json::from_str(data).map_err(|e| format!("json: {e}"))?;
            let msg = v
                .get("error")
                .and_then(|e| e.as_str())
                .or_else(|| v.get("message").and_then(|m| m.as_str()))
                .unwrap_or("Unknown error")
                .to_string();
            Some(CliEvent {
                cli_id: CliId::Opencode,
                event_type: CliEventType::Failed,
                prompt_label: v
                    .get("title")
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string()),
                error_message: Some(msg),
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            })
        }
        "session.created" | "server.connected" | "session.deleted" => {
            // Ignore — not state changes we track
            None
        }
        _ => None,
    };

    if let Some(event) = cli_event {
        if let Ok(mut agg) = aggregator.lock() {
            agg.ingest(event);
        }
    }
    Ok(())
}
