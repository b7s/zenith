//! Tauri command adapters for the calendar-sync domain.
//!
//! Thin wrappers: receive args, call pure services, return results. No
//! business logic here.

use tauri::AppHandle;
use tauri::Emitter;

use crate::calendar_sync::accounts::{load_accounts, remove_account, save_accounts};
use crate::calendar_sync::model::{CalendarAccount, PendingAuthStatus};
use crate::calendar_sync::oauth;
use crate::calendar_sync::poll;
use crate::calendar_sync::sync;

/// List all configured calendar accounts (tokens are DPAPI-wrapped blobs,
/// never returned in plaintext to the frontend).
#[tauri::command]
pub fn calendar_accounts_list() -> Vec<CalendarAccount> {
    load_accounts()
}

/// Begin an OAuth connection for `provider` ("google" | "outlook").
/// Returns `(pending_id, authorize_url)`. The frontend opens the URL in the
/// browser, then polls `calendar_poll_auth` until it resolves.
#[tauri::command]
pub fn calendar_connect(provider: String) -> Result<(String, String), String> {
    oauth::begin_flow(&provider)
}

/// Poll the status of an in-flight OAuth flow.
#[tauri::command]
pub fn calendar_poll_auth(pending_id: String) -> PendingAuthStatus {
    oauth::poll_pending(&pending_id)
}

/// Abort an in-flight OAuth flow (e.g. user closed the dialog).
#[tauri::command]
pub fn calendar_abort_auth(pending_id: String) {
    oauth::abort_flow(&pending_id);
}

/// Disconnect an account and delete all events it produced.
#[tauri::command]
pub fn calendar_disconnect(app: AppHandle, account_id: String) -> Result<(), String> {
    remove_account(Some(&app), &account_id)?;
    app.emit(crate::shared::EVENT_EVENTS_UPDATED, ()).ok();
    Ok(())
}

/// Force an immediate sync of all enabled accounts.
#[tauri::command]
pub fn calendar_sync_now() -> usize {
    let n = sync::sync_all();
    poll::trigger_now();
    n
}

/// Persist account edits made in the widget-config UI (toggle enabled,
/// change label, change poll interval). Tokens are left untouched.
#[tauri::command]
pub fn calendar_save_accounts(app: AppHandle, accounts: Vec<CalendarAccount>) -> Result<(), String> {
    save_accounts(&app, &accounts)?;
    app.emit(crate::shared::EVENT_EVENTS_UPDATED, ()).ok();
    Ok(())
}

/// Set just the enabled flag on one account (quick toggle from the UI).
#[tauri::command]
pub fn calendar_set_enabled(app: AppHandle, account_id: String, enabled: bool) -> Result<(), String> {
    let mut accounts = load_accounts();
    if let Some(acc) = accounts.iter_mut().find(|a| a.id == account_id) {
        acc.enabled = enabled;
    } else {
        return Err(format!("account {account_id} not found"));
    }
    save_accounts(&app, &accounts)?;
    let _ = sync::sync_account(&account_id);
    app.emit(crate::shared::EVENT_EVENTS_UPDATED, ()).ok();
    Ok(())
}
