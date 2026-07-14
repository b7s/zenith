//! Zenith events — local file store + optional OneDrive sync.
//!
//! Storage layout:
//!   - Local:  `%APPDATA%\zenith\calendar-events.json`
//!   - Remote: `<OneDrive>\Zenith\calendar-events.json`     (only when sync enabled)
//!
//! The local file is the canonical source of truth on disk. Startup sync picks
//! the freshest of the two (by max `updated_at`) and saves the loser side. Both
//! the in-memory Vec and the file are always-merging-friendly: unknown keys
//! are preserved, missing events append to the existing list.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use super::model::{CalendarEvent, Recurrence};
use crate::config::repository as cfg_repo;
use crate::shared::known_folders;

/// One config key path used to read the onedrive sync toggle.
const CFG_KEY_ONEDRIVE: &str = "/widgets/config/datetime/onedrive_sync_enabled";

const ONEDRIVE_SUBFOLDER: &str = "Zenith";
const FILE_NAME: &str = "calendar-events.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct EventsFile {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub events: Vec<CalendarEvent>,
}

fn default_version() -> u32 {
    1
}

// ----- paths ----------------------------------------------------------------

pub fn local_path() -> PathBuf {
    crate::config::repository::config_dir().join(FILE_NAME)
}

pub fn onedrive_path() -> Option<PathBuf> {
    known_folders::onedrive_path()
        .map(|p| p.join(ONEDRIVE_SUBFOLDER).join(FILE_NAME))
}

#[allow(dead_code)]
pub fn events_path() -> PathBuf {
    onedrive_path().unwrap_or_else(local_path)
}

// ----- IO -------------------------------------------------------------------

fn read_file(path: &Path) -> Result<Option<EventsFile>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let v: EventsFile = serde_json::from_str(&raw)
        .map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(Some(v))
}

fn write_file(path: &Path, file: &EventsFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename → {}: {e}", path.display()))?;
    Ok(())
}

fn max_updated(events: &[CalendarEvent]) -> i64 {
    events.iter().map(|e| e.updated_at).max().unwrap_or(0)
}

// ----- public API -----------------------------------------------------------

/// Load all events, preferring local file. Always returns a usable Vec.
pub fn load() -> Vec<CalendarEvent> {
    match read_file(&local_path()) {
        Ok(Some(file)) => file.events,
        _ => Vec::new(),
    }
}

pub fn upsert(event: CalendarEvent) -> Result<(), String> {
    upsert_many(std::iter::once(event))
}

/// Upsert many events in a single read/modify/write pass (used by calendar
/// sync, which pulls a batch per account). Matching is by `id`; new ids
/// are appended. The list is re-sorted by date/time afterwards.
pub fn upsert_many(events: impl IntoIterator<Item = CalendarEvent>) -> Result<(), String> {
    let mut file = read_file(&local_path())?.unwrap_or_default();
    for event in events {
        if let Some(slot) = file.events.iter_mut().find(|e| e.id == event.id) {
            *slot = event;
        } else {
            file.events.push(event);
        }
    }
    file.events.sort_by(|a, b| a.date.cmp(&b.date).then(a.time.cmp(&b.time)));
    write_file(&local_path(), &file)?;
    Ok(())
}

pub fn delete_by_id(id: &str) -> Result<bool, String> {
    let mut file = match read_file(&local_path())? {
        Some(f) => f,
        None => return Ok(false),
    };
    let before = file.events.len();
    file.events.retain(|e| e.id != id);
    if file.events.len() == before {
        return Ok(false);
    }
    write_file(&local_path(), &file)?;
    Ok(true)
}

/// Delete every event authored by a specific calendar account (used when
/// the user disconnects that account, so its synced events don't linger
/// on the calendar grid / alarms widget). Returns the number removed.
pub fn delete_by_source_account(account_id: &str) -> usize {
    match read_file(&local_path()) {
        Ok(Some(mut file)) => {
            let before = file.events.len();
            file.events.retain(|e| e.source_account_id != account_id);
            let removed = before - file.events.len();
            if removed > 0 {
                let _ = write_file(&local_path(), &file);
            }
            removed
        }
        _ => 0,
    }
}

/// Delete every event originating from a given provider (`"google"` /
/// `"outlook"`). Used during development / manual resets.
#[allow(dead_code)]
pub fn delete_by_source(source: &str) -> usize {
    match read_file(&local_path()) {
        Ok(Some(mut file)) => {
            let before = file.events.len();
            file.events.retain(|e| e.source != source);
            let removed = before - file.events.len();
            if removed > 0 {
                let _ = write_file(&local_path(), &file);
            }
            removed
        }
        _ => 0,
    }
}

/// Stamp `last_notified_at` on a single event row. Used by the
/// alarm-fire thread to mark that an event-start notification just
/// fired for this occurrence, so the next 30-second tick won't
/// re-popup the same row.
pub fn mark_event_notified(id: &str, when_secs: i64) -> Result<bool, String> {
    let mut file = match read_file(&local_path())? {
        Some(f) => f,
        None => return Ok(false),
    };
    let mut touched = false;
    for ev in file.events.iter_mut() {
        if ev.id == id {
            ev.last_notified_at = when_secs;
            touched = true;
            break;
        }
    }
    if !touched {
        return Ok(false);
    }
    write_file(&local_path(), &file)?;
    Ok(true)
}

/// Startup sync: read local + OneDrive, keep the fresher set.
///
/// Resolution order:
///   1. Both exist → take max(updated_at), push loser.
///
///   2. Only one exists → use that one, push to other (if enabled).
///
///   3. Neither → leave empty.
pub fn startup_sync(app: &AppHandle) {
    let _ = run_startup_sync(app);
}

fn run_startup_sync(app: &AppHandle) -> Result<(), String> {
    let local = match read_file(&local_path()) {
        Ok(opt) => opt,
        Err(e) => {
            eprintln!("[zenith:events] local read failed: {e}");
            None
        }
    };
    let remote_path = onedrive_path();
    let remote_enabled = onedrive_enabled(app);
    let remote = if remote_enabled {
        remote_path
            .as_ref()
            .and_then(|p| match read_file(p) {
                Ok(Some(f)) => Some((p.clone(), f)),
                Ok(None) => None,
                Err(e) => {
                    eprintln!("[zenith:events] onedrive read failed: {e}");
                    None
                }
            })
    } else {
        None
    };

    match (local, remote) {
        (Some(loc), Some((path, rem))) => {
            let lu = max_updated(&loc.events);
            let ru = max_updated(&rem.events);
            if lu >= ru {
                // local is newer (or equal)
                write_file(&path, &loc)?;
            } else {
                // remote is newer
                write_file(&local_path(), &rem)?;
            }
        }
        (Some(loc), None) => {
            // Push local to OneDrive if sync enabled.
            if remote_enabled {
                if let Some(path) = remote_path {
                    let _ = write_file(&path, &loc);
                }
            }
        }
        (None, Some((path, rem))) => {
            // Remote only; copy to local.
            write_file(&local_path(), &rem)?;
            let _ = path; // silence unused
        }
        (None, None) => {
            // Nothing exists anywhere. Leave empty.
        }
    }
    Ok(())
}

/// Force a push to OneDrive (caller already wrote the local).
/// Returns false if sync disabled or OneDrive unavailable.
pub fn force_sync(app: &AppHandle) -> Result<bool, String> {
    if !onedrive_enabled(app) {
        return Ok(false);
    }
    let path = match onedrive_path() {
        Some(p) => p,
        None => return Ok(false),
    };
    let local = read_file(&local_path())?.unwrap_or_default();
    write_file(&path, &local)?;
    Ok(true)
}

fn onedrive_enabled(_app: &AppHandle) -> bool {
    cfg_repo::get_or(CFG_KEY_ONEDRIVE, false)
}

// ----- mutations used by background tasks -----------------------------------

/// Delete events that have aged past `max_age_days`.
///
/// Only deletes `Recurrence::None` events (one-shots). Recurring events are
/// kept regardless of age — the user may want to keep weekly repeating
/// events long-term. Returns the number of events removed.
pub fn cleanup_old_events(max_age_days: u32) -> Result<usize, String> {
    let mut file = match read_file(&local_path())? {
        Some(f) => f,
        None => return Ok(0),
    };
    let before = file.events.len();
    let today_days = epoch_days(today_seconds());
    let cutoff = today_days.saturating_sub(max_age_days as i64);
    file.events.retain(|e| {
        if e.recurrence != Recurrence::None {
            return true;
        }
        let ed = parse_date(&e.date).unwrap_or(0);
        ed >= cutoff
    });
    let removed = before - file.events.len();
    if removed > 0 {
        write_file(&local_path(), &file)?;
    }
    Ok(removed)
}

// ----- utilities ------------------------------------------------------------

/// Parse "YYYY-MM-DD" into days-since-epoch, or 0 on failure.
fn parse_date(s: &str) -> Result<i64, ()> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return Err(());
    }
    let y: i64 = parts[0].parse().map_err(|_| ())?;
    let m: i64 = parts[1].parse().map_err(|_| ())?;
    let d: i64 = parts[2].parse().map_err(|_| ())?;
    Ok(civil_to_days(y, m, d))
}

fn civil_to_days(y: i64, m: i64, d: i64) -> i64 {
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719469
}

fn epoch_days(secs: i64) -> i64 {
    secs / 86400
}

fn today_seconds() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
