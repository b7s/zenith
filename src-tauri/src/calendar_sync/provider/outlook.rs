//! Microsoft Outlook / Graph calendar provider.
//!
//! Uses the Graph `calendarView` (expand/recurring events, ordered by
//! start). Public client (PKCE `code_challenge_method=plain`) — no client
//! secret stored.

use crate::calendar_sync::accounts::{access_token_plain, refresh_token_plain, update_account, wrap_tokens};
use crate::calendar_sync::credentials;
use crate::calendar_sync::iso8601;
use crate::calendar_sync::model::CalendarAccount;
use crate::calendar_sync::provider::{provider_for, ProviderApi, build_event, SyncWindow, token_post};
use crate::events::model::CalendarEvent;

pub struct OutlookProvider;

impl ProviderApi for OutlookProvider {
    fn refresh(&self, account: &CalendarAccount) -> Result<(String, i64), String> {
        let refresh = refresh_token_plain(account);
        if refresh.is_empty() {
            return Err("no refresh token stored".into());
        }
        let body = vec![
            ("client_id", credentials::outlook::CLIENT_ID.to_string()),
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh),
            ("scope", credentials::outlook::SCOPES.to_string()),
        ];
        let val = token_post(credentials::outlook::TOKEN_URL, &body)?;
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
            "{base}?startDateTime={min}&endDateTime={max}&$select=id,subject,bodyPreview,start,end,isAllDay&$orderby=start/dateTime&$top=250",
            base = credentials::outlook::EVENTS_URL,
            min = iso8601::to_rfc3339(window.start),
            max = iso8601::to_rfc3339(window.end),
        );
        let resp = ureq::get(&url)
            .set("Authorization", &format!("Bearer {access}"))
            .call()
            .map_err(|e| format!("outlook events request failed: {e}"))?;
        let val: serde_json::Value = resp
            .into_json()
            .map_err(|e| format!("outlook events not json: {e}"))?;
        let items = val.get("value").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let external_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let title = item
                .get("subject")
                .and_then(|v| v.as_str())
                .unwrap_or("(no title)")
                .to_string();
            let notes = item
                .get("bodyPreview")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let start = item.get("start");
            let end = item.get("end");
            let all_day = item
                .get("isAllDay")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let (start_secs, end_secs) = parse_graph_bounds(start, end, all_day);
            if start_secs == 0 {
                continue;
            }
            out.push(build_event(
                credentials::outlook::SOURCE,
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
    let p = provider_for(account);
    let (access, expires_in) = p.refresh(account)?;
    let refreshed = wrap_tokens(account, &access, expires_in);
    let _ = update_account(None, &refreshed);
    Ok((refreshed, access))
}

/// Graph stores `start`/`end` as `{ "dateTime": "..", "timeZone": ".." }`.
/// `dateTime` is already in the user's local zone — to keep timestamps
/// stable we interpret them as the offset present (Graph emits an offset
/// like `2024-01-01T09:00:00-05:00` when a zone is given, or UTC `Z`).
fn parse_graph_bounds(
    start: Option<&serde_json::Value>,
    end: Option<&serde_json::Value>,
    all_day: bool,
) -> (i64, i64) {
    let start_secs = start
        .and_then(|s| s.get("dateTime"))
        .and_then(|v| v.as_str())
        .map(iso8601::parse_rfc3339)
        .unwrap_or(0);
    let end_secs = end
        .and_then(|s| s.get("dateTime"))
        .and_then(|v| v.as_str())
        .map(iso8601::parse_rfc3339)
        .unwrap_or(start_secs);
    // All-day events from Graph carry a `dateTime` of `00:00:00` in the
    // user's tz; treat end-exclusive by adding a day if it equals start.
    if all_day && end_secs <= start_secs {
        (start_secs, start_secs + 86400)
    } else {
        (start_secs, end_secs)
    }
}
