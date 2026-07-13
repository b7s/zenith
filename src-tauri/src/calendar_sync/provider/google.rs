//! Google Calendar provider.
//!
//! Uses the Calendar API `events.list` endpoint with `singleEvents=true`
//! so recurring events are expanded into concrete instances. Public
//! client (PKCE `code_challenge_method=plain`) — no client secret stored.

use crate::calendar_sync::accounts::{
    access_token_plain, refresh_token_plain, update_account, wrap_tokens,
};
use crate::calendar_sync::credentials;
use crate::calendar_sync::iso8601;
use crate::calendar_sync::model::CalendarAccount;
use crate::calendar_sync::provider::{provider_for, SyncWindow, ProviderApi, build_event, token_post};
use crate::events::model::CalendarEvent;

pub struct GoogleProvider;

impl ProviderApi for GoogleProvider {
    fn refresh(&self, account: &CalendarAccount) -> Result<(String, i64), String> {
        let refresh = refresh_token_plain(account);
        if refresh.is_empty() {
            return Err("no refresh token stored".into());
        }
        let body = vec![
            ("client_id", credentials::google::CLIENT_ID.to_string()),
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh),
        ];
        let val = token_post(credentials::google::TOKEN_URL, &body)?;
        let access = val
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or("missing access_token")?
            .to_string();
        let expires_in = val
            .get("expires_in")
            .and_then(|v| v.as_i64())
            .unwrap_or(3600);
        Ok((access, expires_in))
    }

    fn fetch(&self, account: &CalendarAccount, window: &SyncWindow) -> Result<Vec<CalendarEvent>, String> {
        let (account, access) = ensure_fresh(account)?;
        let url = format!(
            "{base}?timeMin={min}&timeMax={max}&singleEvents=true&orderBy=startTime&maxResults=250",
            base = credentials::google::EVENTS_URL,
            min = iso8601::to_rfc3339(window.start),
            max = iso8601::to_rfc3339(window.end),
        );
        let resp = ureq::get(&url)
            .set("Authorization", &format!("Bearer {access}"))
            .call()
            .map_err(|e| format!("google events request failed: {e}"))?;
        let val: serde_json::Value = resp
            .into_json()
            .map_err(|e| format!("google events not json: {e}"))?;
        let items = val.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let external_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = item
                .get("summary")
                .and_then(|v| v.as_str())
                .or_else(|| item.get("title").and_then(|v| v.as_str()))
                .unwrap_or("(no title)")
                .to_string();
            let notes = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let start = item.get("start");
            let end = item.get("end");
            let (start_secs, end_secs, all_day) = parse_bounds(start, end);
            if start_secs == 0 {
                continue;
            }
            out.push(build_event(
                credentials::google::SOURCE,
                &account.id,
                &external_id,
                &title,
                &notes,
                start_secs,
                end_secs,
                all_day,
            ));
        }
        Ok(out)
    }
}

/// Return a (possibly refreshed) account + plaintext access token.
fn ensure_fresh(account: &CalendarAccount) -> Result<(CalendarAccount, String), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if account.expires_at > now + 60 {
        return Ok((account.clone(), access_token_plain(account)));
    }
    // Refresh.
    let p = provider_for(account);
    let (access, expires_in) = p.refresh(account)?;
    let refreshed = wrap_tokens(account, &access, expires_in);
    let _ = update_account(None, &refreshed);
    Ok((refreshed, access))
}

/// Extract start/end seconds + all-day flag from a Google event bounds
/// object (`{ "dateTime": ".." }` or `{ "date": ".." }`).
fn parse_bounds(start: Option<&serde_json::Value>, end: Option<&serde_json::Value>) -> (i64, i64, bool) {
    let mut all_day = false;
    let start_secs = match start.and_then(|s| s.get("dateTime")).and_then(|v| v.as_str()) {
        Some(dt) => iso8601::parse_rfc3339(dt),
        None => match start.and_then(|s| s.get("date")).and_then(|v| v.as_str()) {
            Some(d) => {
                all_day = true;
                iso8601::parse_rfc3339(d)
            }
            None => 0,
        },
    };
    let end_secs = match end.and_then(|s| s.get("dateTime")).and_then(|v| v.as_str()) {
        Some(dt) => iso8601::parse_rfc3339(dt),
        None => match end.and_then(|s| s.get("date")).and_then(|v| v.as_str()) {
            Some(d) => {
                // date-only end is exclusive (next day); subtract a day.
                iso8601::parse_rfc3339(d) - 86400
            }
            None => start_secs,
        },
    };
    (start_secs, end_secs, all_day)
}
