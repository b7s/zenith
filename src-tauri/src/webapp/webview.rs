//! Window lifecycle watcher for the webapp link windows.
//!
//! Spawns a background thread per link window that periodically checks if the
//! Tauri window still exists. When the window opens, it emits
//! `zenith:link-notification { id, has: true }` (dot appears on the bar icon).
//! When the window is destroyed/closed, it emits `{ id, has: false }`
//! (dot disappears). The thread exits after emitting the closed state.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::shared::EVENT_LINK_NOTIFICATION;

/// How often to check whether the link window still exists.
const POLL_MS: u64 = 2000;

pub fn parse_badge(title: &str) -> bool {
    let t = title.trim();
    if t.is_empty() {
        return false;
    }
    if t.contains('\u{25CF}') {
        return true;
    }
    if let Some(rest) = t.strip_prefix('(') {
        if let Some(num) = rest.split(')').next() {
            if let Ok(n) = num.trim().parse::<i32>() {
                return n > 0;
            }
        }
    }
    let lower = t.to_lowercase();
    if lower.contains("unread") || lower.contains("new message") {
        if let Some(n) = t
            .split_whitespace()
            .next()
            .and_then(|w| w.parse::<i32>().ok())
        {
            return n > 0;
        }
        return true;
    }
    false
}

/// Start watching a link window's lifecycle. Emits `zenith:link-notification`
/// with `has: true` immediately (dot appears), then polls every 2s. When the
/// window goes away (closed/destroyed) it emits `has: false` (dot disappears)
/// and the thread exits.
///
/// Pass `stop` to force-stop the watcher early (e.g. on teardown).
pub fn watch_lifecycle(
    app: AppHandle,
    window_label: String,
    link_id: String,
    stop: Arc<AtomicBool>,
) {
    // Emit "open" immediately.
    let _ = app.emit(
        EVENT_LINK_NOTIFICATION,
        serde_json::json!({ "id": &link_id, "has": true }),
    );

    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(POLL_MS));
            let alive = app.get_webview_window(&window_label).is_some();
            if !alive {
                let _ = app.emit(
                    EVENT_LINK_NOTIFICATION,
                    serde_json::json!({ "id": &link_id, "has": false }),
                );
                return;
            }
        }
        // Stopped early — emit closed so the dot goes away even on forced
        // teardown (the poll 2s timer may not have fired yet).
        let _ = app.emit(
            EVENT_LINK_NOTIFICATION,
            serde_json::json!({ "id": &link_id, "has": false }),
        );
    });
}
