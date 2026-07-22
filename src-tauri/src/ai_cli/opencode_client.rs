//! Process-detection fallback for opencode.
//!
//! When the opencode JS plugin is installed and loaded, events flow through
//! the bridge server directly. This module provides a safety-net: if opencode.exe
//! is running but no events have been seen through the plugin (e.g. plugin not
//! registered or opencode loaded from a different config), we inject a basic
//! "running" state so the bar widget still shows a blue dot.
//!
//! Polls every 5–15 s, comparing the aggregator state with actual process presence.
//! Only emits changes when a mismatch is detected, so the plugin events take priority.

use std::sync::Mutex;
use std::time::Duration;

use tauri::Emitter;

use super::aggregator::Aggregator;
use super::model::{CliEvent, CliEventType, CliId};

/// Check if `opencode.exe` is running via Windows process enumeration.
fn is_opencode_process_running() -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::{EnumProcesses, GetModuleBaseNameW};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    let mut pids = vec![0u32; 4096];
    let mut bytes_returned: u32 = 0;
    unsafe {
        let _ = EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut bytes_returned);
    }
    let count = (bytes_returned as usize) / 4;
    if count == 0 {
        return false;
    }
    let mut buf = vec![0u16; 260];
    for &pid in &pids[..count.min(pids.len())] {
        if pid == 0 {
            continue;
        }
        unsafe {
            if let Ok(handle) = OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
                false,
                pid,
            ) {
                let len = GetModuleBaseNameW(handle, None, buf.as_mut_slice()) as usize;
                let _ = CloseHandle(handle);
                if len > 0 {
                    let name = String::from_utf16_lossy(&buf[..len]);
                    if name.eq_ignore_ascii_case("opencode.exe") {
                        return true;
                    }
                    // Log first few matching "open*" processes for diagnostics
                    if name.to_ascii_lowercase().starts_with("open") {
                        eprintln!("[zenith:ai-cli:process] matching process pid={pid} name={name:?}");
                    }
                }
            }
        }
    }
    false
}

/// Spawn the process-detection fallback thread.
/// Compares aggregator state with actual opencode process presence every 5–15 s.
pub fn spawn(aggregator: std::sync::Arc<Mutex<Aggregator>>) {
    std::thread::spawn(move || {
        eprintln!("[zenith:ai-cli:process] fallback started");
        loop {
            let running = is_opencode_process_running();

            let current_running = aggregator
                .lock()
                .ok()
                .and_then(|a| {
                    let snapshots = a.aggregate().per_cli;
                    snapshots
                        .iter()
                        .find(|s| s.cli_id == "opencode")
                        .map(|s| s.is_running)
                })
                .unwrap_or(false);

            if running != current_running {
                eprintln!(
                    "[zenith:ai-cli:process] mismatch: process={running} aggregator={current_running} → injecting {}",
                    if running { "Started" } else { "Idle" }
                );
                let event_type = if running {
                    CliEventType::Started
                } else {
                    CliEventType::Idle
                };
                if let Ok(mut agg) = aggregator.lock() {
                    agg.ingest(CliEvent {
                        cli_id: CliId::Opencode,
                        event_type,
                        prompt_label: None,
                        error_message: None,
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    });
                }
                if let Some(h) = crate::shared::app_handle() {
                    if let Ok(agg) = aggregator.lock() {
                        let state = agg.aggregate();
                        eprintln!("[zenith:ai-cli:process] emitting state: any_running={}", state.any_running);
                        let _ = h.emit(crate::shared::EVENT_AI_CLI_CHANGED, &state);
                    }
                } else {
                    eprintln!("[zenith:ai-cli:process] app_handle not available");
                }
                std::thread::sleep(Duration::from_secs(5));
            } else {
                eprintln!("[zenith:ai-cli:process] in-sync: process={running} aggregator={current_running}");
                std::thread::sleep(Duration::from_secs(15));
            }
        }
    });
}
