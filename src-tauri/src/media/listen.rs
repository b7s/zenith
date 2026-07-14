//! Background watcher for the SMTC current-session state.
//!
//! Per AGENTS.md §13.1, ALL SMTC work runs off the Tauri main thread — the
//! poll thread is the single writer of `commands::CACHED`. Widget code reads
//! the snapshot either from the cache (via `get_media`) or via the
//! `zenith:media-changed` event payload (which the poll thread emits with
//! the snapshot so the frontend doesn't even need to IPC back to read the
//! new state).
//!
//! Poll cadence is 2 s. Dedup key: title|artist|status|duration|
//! 100-ms-rounded-position|rate — so a 60-second playback emits only once
//! at start (unless the user seeks/pauses etc.).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use super::commands::{capture_session, cache_set, resolve_current, MediaSnapshot};
static LAST_KEY: Mutex<String> = Mutex::new(String::new());
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Emit the snapshot so frontend listeners don't need to round-trip back
/// through `get_media`. Payload shape mirrors the IPC command.
pub fn fire_changed(app: &AppHandle, snap: &MediaSnapshot) {
    let _ = app.emit(crate::shared::EVENT_MEDIA_CHANGED, snap);
}

/// Start the polling thread. Idempotent (guarded by `RUNNING`). The thread
/// runs forever but bails out of SMTC work when no bar window exists.
pub fn spawn(app: AppHandle) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        // COM init for this worker thread — required for SMTC.
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
        loop {
            // First iteration runs immediately so the bar is populated on
            // startup instead of waiting up to 2 s for the cache.
            if app.get_webview_window("bar").is_none() {
                std::thread::sleep(Duration::from_millis(2000));
                continue;
            }
            let snap = match resolve_current() {
                Some(s) => {
                    let info = capture_session(&s);
                    MediaSnapshot { available: info.is_some(), info }
                }
                None => MediaSnapshot { available: false, info: None },
            };
            let key = match &snap.info {
                Some(info) => format!(
                    "{}|{}|{}|{}|{}ms|{:.3}",
                    info.title, info.artist, info.status,
                    info.duration_ms,
                    (info.position_ms / 100).max(0),
                    info.rate,
                ),
                None => String::new(),
            };

            let changed = {
                match LAST_KEY.lock() {
                    Ok(mut last) => {
                        if last.as_str() == key.as_str() {
                            false
                        } else {
                            last.clear();
                            last.push_str(&key);
                            true
                        }
                    }
                    Err(_) => false,
                }
            };

            cache_set(Some(snap.clone()));

            if changed {
                fire_changed(&app, &snap);
            }

            std::thread::sleep(Duration::from_millis(2000));
        }
    });
}
