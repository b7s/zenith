use tauri::{AppHandle, Emitter};

use super::model::CalendarEvent;
use super::repository;

fn emit_updated(app: &AppHandle) {
    let _ = app.emit(crate::shared::EVENT_EVENTS_UPDATED, ());
}

/// Return all events sorted by date+time ascending.
#[tauri::command]
pub fn get_events() -> Vec<CalendarEvent> {
    repository::load()
}

#[tauri::command]
pub fn add_event(app: AppHandle, mut event: CalendarEvent) -> Result<CalendarEvent, String> {
    if event.id.is_empty() {
        event.id = uuid_v4_like();
    }
    let now = now_secs();
    event.created_at = now;
    event.updated_at = now;
    repository::upsert(event.clone()).map_err(|e| e.to_string())?;
    emit_updated(&app);
    Ok(event)
}

#[tauri::command]
pub fn update_event(app: AppHandle, mut event: CalendarEvent) -> Result<CalendarEvent, String> {
    if event.id.is_empty() {
        return Err("update_event: missing id".into());
    }
    event.updated_at = now_secs();
    repository::upsert(event.clone()).map_err(|e| e.to_string())?;
    emit_updated(&app);
    Ok(event)
}

#[tauri::command]
pub fn delete_event(app: AppHandle, id: String) -> Result<bool, String> {
    let removed = repository::delete_by_id(&id).map_err(|e| e.to_string())?;
    if removed {
        emit_updated(&app);
    }
    Ok(removed)
}

/// Force a sync with OneDrive (if enabled in config).
#[tauri::command]
pub fn sync_events(app: AppHandle) -> Result<(), String> {
    repository::force_sync(&app).map_err(|e| e.to_string())?;
    emit_updated(&app);
    Ok(())
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Tiny RFC-4122 v4 UUID without the `uuid` crate.
///
/// Uses `SystemTime::now().as_nanos()` as a deterministic-ish random source
/// — good enough for local file-scoped IDs. If you need cryptographic
/// entropy, replace with the `uuid` crate later.
fn uuid_v4_like() -> String {
    use std::time::SystemTime;
    let n = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let lo = n as u64;
    let hi = (n >> 64) as u64;
    // Mix the two halves with a tiny splitmix64-like smear.
    let mut seed: u64 = lo ^ hi.rotate_left(17);
    let mut bytes = [0u8; 16];
    for slot in bytes.iter_mut() {
        seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        let v = z ^ (z >> 31);
        *slot = (v & 0xff) as u8;
    }
    // RFC4122 variant + version bits
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}
