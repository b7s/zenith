//! Update checker — polls the GitHub "latest release" API for the
//! `b7s/zenith` repo and compares the returned tag against the running
//! build version. The check runs once every 12 hours (when `auto_update`
//! is on) and on demand via `check_update`. When a newer version exists,
//! the `zenith:update-available` event is emitted with the latest tag.
//!
//! `ureq` (blocking) is the project's existing HTTP client. Commands run
//! on Tauri's worker threads, so the blocking call never stalls the main
//! IPC channel. The 12h loop runs on its own thread and reads config
//! each cycle, so toggling `auto_update` takes effect without a restart.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::config;
use crate::shared::shell;
use crate::shared::{EVENT_UPDATE_AVAILABLE, EVENT_UPDATE_CHECKED};

const RELEASES_API: &str = "https://api.github.com/repos/b7s/zenith/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/b7s/zenith/releases";

#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateStatus {
    /// True when a newer version than the running build was found.
    pub update_available: bool,
    /// Latest release tag (e.g. "v0.1.4"); empty on error / no data.
    pub latest_version: String,
    /// Running build version (e.g. "0.1.3").
    pub current_version: String,
    /// Human-readable result (e.g. "Up to date", or an error string).
    pub message: String,
    /// Unix seconds of the last completed check (success or failure).
    pub checked_at: i64,
}

static STATUS: OnceLock<Mutex<UpdateStatus>> = OnceLock::new();

fn status() -> &'static Mutex<UpdateStatus> {
    STATUS.get_or_init(|| Mutex::new(UpdateStatus::default()))
}

/// Borrow the current cached status (used by the settings UI on open).
#[tauri::command]
pub fn get_update_status() -> UpdateStatus {
    status().lock().map(|g| g.clone()).unwrap_or_default()
}

/// Open the releases page in the default browser.
#[tauri::command]
pub fn open_releases_page() -> Result<(), String> {
    if shell::open_url(RELEASES_PAGE) {
        Ok(())
    } else {
        Err("failed to open browser".to_string())
    }
}

/// Trim a leading `v`/`V` and any build metadata, returning a comparable
/// `[major, minor, patch]` triple.
fn parse_version(tag: &str) -> Option<(u32, u32, u32)> {
    let t = tag.trim().trim_start_matches(['v', 'V']);
    let core = t.split(['-', '+']).next().unwrap_or(t);
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
    let patch = parts.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
    Some((major, minor, patch))
}

/// Compare `latest` against `current`: >0 if latest is newer.
fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Fetch the latest release tag from GitHub. Returns the raw tag string.
fn fetch_latest_tag() -> Result<String, String> {
    let resp = ureq::get(RELEASES_API)
        .set("User-Agent", "zenith")
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(15))
        .call()
        .map_err(|e| format!("network: {e}"))?;
    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("parse: {e}"))?;
    v.get("tag_name")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "no tag_name in response".to_string())
}

/// Perform a single check. Updates the cached status and emits events.
/// Returns the resulting status.
pub fn run_check(app: &AppHandle) -> UpdateStatus {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let mut next = UpdateStatus {
        current_version: current.clone(),
        ..Default::default()
    };

    match fetch_latest_tag() {
        Ok(tag) => {
            next.latest_version = tag.clone();
            if is_newer(&tag, &current) {
                next.update_available = true;
                next.message = format!("Version {} is available", tag);
            } else {
                next.message = "Up to date".to_string();
            }
        }
        Err(e) => {
            next.message = format!("Check failed: {e}");
        }
    }
    next.checked_at = now_secs();

    // Persist into cache, then broadcast.
    if let Ok(mut g) = status().lock() {
        *g = next.clone();
    }
    let _ = app.emit(EVENT_UPDATE_CHECKED, &next);
    if next.update_available {
        let _ = app.emit(
            EVENT_UPDATE_AVAILABLE,
            serde_json::json!({ "version": next.latest_version, "url": RELEASES_PAGE }),
        );
    }
    next
}

/// Manual check command (frontend "Check now" button).
#[tauri::command]
pub fn check_update(app: AppHandle) -> UpdateStatus {
    run_check(&app)
}

/// Spawn the 12-hour background loop. Each cycle re-reads config, so the
/// `auto_update` toggle (saved to config) takes effect immediately.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last_check = 0i64;
        loop {
            std::thread::sleep(Duration::from_secs(60));
            let cfg = config::load();
            if !cfg.updates.auto_update {
                continue;
            }
            let now = now_secs();
            // First run shortly after launch, then every 12h.
            if last_check == 0 || now - last_check >= 12 * 3600 {
                last_check = now;
                run_check(&app);
            }
        }
    });
}
