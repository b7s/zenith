//! Generic OneDrive file-sync primitives shared by all Zenith domains
//! (config, events, future ones).
//!
//! Each domain owns its own local path resolution (`config::repository::config_path`,
//! `events::repository::local_path`); this module owns the **remote path
//! shape** (`<OneDrive>/Zenith/<name>`) plus the read/write/sync primitives
//! that work on any `Serialize`/`DeserializeOwned` type. Both the config
//! domain and the events domain call into these so the IO + atomic-write +
//! remote-push logic lives in exactly one place.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};

use super::known_folders;

/// Subfolder under the user's OneDrive where Zenith data files live.
/// Shared by every domain so a single OneDrive backup folder holds all
/// Zenith state (config.json, calendar-events.json, …).
const ONEDRIVE_SUBFOLDER: &str = "Zenith";

/// OneDrive path for a Zenith data file: `<OneDrive>/Zenith/<name>`, or
/// `None` when OneDrive is not available on this machine.
///
/// `name` is the bare file name (e.g. `"config.json"`, `"calendar-events.json"`).
pub fn onedrive_path_for(name: &str) -> Option<PathBuf> {
    known_folders::onedrive_path().map(|p| p.join(ONEDRIVE_SUBFOLDER).join(name))
}

/// Read+parse a JSON file. Returns `Ok(None)` if the file is missing.
///
/// Errors (IO or parse) are surfaced as `Err(String)`; callers decide
/// whether to log-and-fallback or propagate.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let v: T =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(Some(v))
}

/// Serialize+atomically write a JSON file. Creates parent dirs.
///
/// Atomicity: writes to `<path>.tmp` then renames over the target. The
/// temp-file approach is crash-safe — a partially-written file never
/// appears at `path`.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    atomic_write(path, json.as_bytes())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut f =
            fs::File::create(&tmp).map_err(|e| format!("create {}: {e}", tmp.display()))?;
        f.write_all(bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
        f.sync_all().map_err(|e| format!("sync {}: {e}", tmp.display()))?;
    }
    fs::rename(&tmp, path).map_err(|e| format!("rename → {}: {e}", path.display()))
}

/// File mtime in epoch seconds, or `None` if the file is missing / its
/// mtime cannot be read. Used by sync resolution to pick the fresher copy.
#[allow(dead_code)]
pub fn mtime_secs(path: &Path) -> Option<i64> {
    use std::time::UNIX_EPOCH;
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

/// Push a value to OneDrive under `<OneDrive>/Zenith/<name>`.
///
/// Returns:
/// - `Ok(false)` when `enabled` is false (no-op, not an error).
/// - `Ok(false)` when OneDrive is unavailable on this machine.
/// - `Ok(true)`  when the file was written.
/// - `Err(_)`    when the write itself failed.
pub fn push_to_onedrive<T: Serialize>(
    name: &str,
    value: &T,
    enabled: bool,
) -> Result<bool, String> {
    if !enabled {
        return Ok(false);
    }
    let path = match onedrive_path_for(name) {
        Some(p) => p,
        None => return Ok(false),
    };
    write_json(&path, value)?;
    Ok(true)
}
