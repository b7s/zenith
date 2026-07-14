//! One-shot calendar sync for a single account.
//!
//! Pulls the account's window of events (from 1 day in the past to 60 days
//! ahead) via the provider, upserts them into the shared events store, and
//! stamps `last_sync_at` / `last_error` on the account.

use crate::calendar_sync::accounts::{load_accounts, update_account};
use crate::calendar_sync::provider::{provider_for, SyncWindow, upsert_events};
use crate::calendar_sync::model::CalendarAccount;

/// How far into the future to pull events on each sync (seconds).
const HORIZON_SECS: i64 = 60 * 24 * 60 * 60; // 60 days
/// How far back to keep events already in progress (seconds).
const LOOKBACK_SECS: i64 = 24 * 60 * 60; // 1 day

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Sync a single account by id. Returns the number of events upserted.
pub fn sync_account(account_id: &str) -> Result<usize, String> {
    let accounts = load_accounts();
    let account = accounts
        .into_iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| format!("account {account_id} not found"))?;
    if !account.enabled {
        return Ok(0);
    }
    sync_one(&account)
}

/// Sync every enabled account. Errors for individual accounts are recorded
/// on the account (`last_error`) and do not abort the others.
pub fn sync_all() -> usize {
    let accounts = load_accounts();
    let mut total = 0;
    for account in accounts.iter().filter(|a| a.enabled) {
        match sync_one(account) {
            Ok(n) => total += n,
            Err(e) => {
                let mut failed = account.clone();
                failed.last_error = e;
                let _ = update_account(None, &failed);
            }
        }
    }
    total
}

fn sync_one(account: &CalendarAccount) -> Result<usize, String> {
    let provider = provider_for(account);
    let now = now_secs();
    let window = SyncWindow {
        start: now - LOOKBACK_SECS,
        end: now + HORIZON_SECS,
    };
    let events = provider.fetch(account, &window).inspect_err(|e| {
        let mut failed = account.clone();
        failed.last_error = e.clone();
        let _ = update_account(None, &failed);
    })?;

    let count = events.len();
    // Upsert + emit on the global app handle when present.
    let app = crate::shared::app_handle();
    upsert_events(events, app.as_ref());

    let mut stamped = account.clone();
    stamped.last_sync_at = now;
    stamped.last_error = String::new();
    let _ = update_account(None, &stamped);
    Ok(count)
}
