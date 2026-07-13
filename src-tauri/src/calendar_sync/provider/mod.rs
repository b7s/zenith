//! Provider abstraction for calendar backends (Google / Outlook).
//!
//! Each provider knows how to (a) refresh its access token and (b) fetch
//! events in a window. Mapping external payloads into `CalendarEvent`
//! happens here. The events store is `date` + `time` (local wall-clock,
//! treated as UTC epoch in `alarm_fire`), so we convert the provider's
//! start/end into those fields. Every synced row is tagged with
//! `source` / `source_account_id` / `external_id`.

pub mod google;
pub mod outlook;

use tauri::Emitter;

use crate::calendar_sync::iso8601;
use crate::calendar_sync::model::CalendarAccount;
use crate::events::model::{CalendarEvent, EventKind};
use crate::events::repository;

/// Window of events to pull on each sync (Unix seconds).
pub struct SyncWindow {
    pub start: i64,
    pub end: i64,
}

pub trait ProviderApi {
    /// Exchange the account's refresh token for a fresh access token.
    /// Returns `(access_token, expires_in_secs)`.
    fn refresh(&self, account: &CalendarAccount) -> Result<(String, i64), String>;
    /// Fetch events in `window` and map into `CalendarEvent`s ready to be
    /// upserted. Each event's `source_account_id` is set by the caller.
    fn fetch(&self, account: &CalendarAccount, window: &SyncWindow) -> Result<Vec<CalendarEvent>, String>;
}

pub fn provider_for(account: &CalendarAccount) -> Box<dyn ProviderApi> {
    use crate::calendar_sync::model::CalendarAccountProvider;
    match account.provider {
        CalendarAccountProvider::Google => Box::new(self::google::GoogleProvider),
        CalendarAccountProvider::Outlook => Box::new(self::outlook::OutlookProvider),
    }
}

/// POST form-encoded to a token URL, returning the parsed JSON.
pub fn token_post(url: &str, body: &[(&str, String)]) -> Result<serde_json::Value, String> {
    let form: Vec<String> = body
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect();
    let resp = ureq::post(url)
        .set("Content-Type", "application/x-www-form-urlencoded")
        .send_string(&form.join("&"))
        .map_err(|e| format!("token request failed: {e}"))?;
    let val: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("token response not json: {e}"))?;
    if let Some(err) = val.get("error").and_then(|e| e.as_str()) {
        return Err(format!("token error: {err}"));
    }
    Ok(val)
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// Build a `CalendarEvent` for a synced external event. `source` is the
/// provider string (`google`/`outlook`). `start_secs`/`end_secs` are Unix
/// seconds (UTC); all-day events get `time = None`. Recurring events fire
/// once per occurrence via `last_notified_at`.
pub fn build_event(
    source: &str,
    account_id: &str,
    external_id: &str,
    title: &str,
    notes: &str,
    start_secs: i64,
    end_secs: i64,
    all_day: bool,
) -> CalendarEvent {
    let id = if external_id.is_empty() {
        format!("{}-{}-{}", source, account_id, start_secs)
    } else {
        format!("{}-{}-{}", source, account_id, external_id)
    };
    let start_rfc = iso8601::to_rfc3339(start_secs);
    let date = start_rfc.get(0..10).unwrap_or("1970-01-01").to_string();
    let time = if all_day {
        None
    } else {
        Some(start_rfc.get(11..16).unwrap_or("00:00").to_string())
    };
    let end_rfc = iso8601::to_rfc3339(end_secs.max(start_secs));
    let end_time = if all_day {
        None
    } else {
        Some(end_rfc.get(11..16).unwrap_or("00:00").to_string())
    };

    CalendarEvent {
        id,
        title: title.to_string(),
        date,
        time,
        end_time,
        kind: EventKind::Event,
        recurrence: crate::events::model::Recurrence::None,
        weekdays: 0,
        enabled: true,
        created_at: start_secs,
        updated_at: start_secs,
        notes: notes.to_string(),
        source: source.to_string(),
        source_account_id: account_id.to_string(),
        external_id: external_id.to_string(),
        notify_on_start: true,
        last_notified_at: 0,
    }
}

/// Upsert the fetched events into the shared store (match by `id`; update
/// in place if present, append otherwise) and emit `events-updated`.
pub fn upsert_events(events: Vec<CalendarEvent>, app: Option<&tauri::AppHandle>) {
    let _ = repository::upsert_many(events);
    if let Some(app) = app {
        let _ = app.emit(crate::shared::EVENT_EVENTS_UPDATED, ());
    }
}

/// RFC3339 helper re-exported for providers.
#[allow(dead_code)]
pub fn fmt(secs: i64) -> String {
    iso8601::to_rfc3339(secs)
}
