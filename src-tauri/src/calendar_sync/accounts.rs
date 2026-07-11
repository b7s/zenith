//! Calendar-account persistence.
//!
//! Accounts live inside `config.json` at
//! `widgets.config.datetime.calendar_accounts` as a `Vec<CalendarAccount>`
//! (JSON). Tokens are DPAPI-wrapped before serialization (handled by the
//! caller — `add_account`/`save_account` receive **plaintext** tokens and
//! wrap them here). This keeps secrets off disk in cleartext.
//!
//! Single home for account CRUD — the oauth flow, the poll thread, and the
//! widget-config UI all go through these functions.

use tauri::Emitter;

use crate::calendar_sync::model::{CalendarAccount, CalendarAccountProvider};
use crate::config::repository as cfg_repo;
use crate::git::secrets;

const CFG_KEY: &str = "/widgets/config/datetime/calendar_accounts";

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Read all accounts from config. Empty vec when none configured.
pub fn load_accounts() -> Vec<CalendarAccount> {
    let cfg = cfg_repo::load();
    let raw = serde_json::to_value(&cfg).unwrap_or(serde_json::Value::Null);
    raw.pointer(CFG_KEY)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Persist the full account list back to config and emit
/// `zenith:config-updated` so every window refreshes.
fn persist(app: &tauri::AppHandle, accounts: &[CalendarAccount]) -> Result<(), String> {
    let mut cfg = cfg_repo::load();
    let datetime = cfg
        .widgets
        .config
        .entry("datetime".to_string())
        .or_default();
    datetime.insert(
        "calendar_accounts".to_string(),
        serde_json::to_value(accounts).map_err(|e| e.to_string())?,
    );
    cfg_repo::save(&cfg).map_err(|e| e.to_string())?;
    app.emit(crate::shared::EVENT_CONFIG_UPDATED, &cfg).ok();
    Ok(())
}

/// Add a brand-new account (used by the OAuth flow). Tokens are
/// DPAPI-wrapped before storage. Returns the new account id.
pub fn add_account(
    provider: CalendarAccountProvider,
    email: &str,
    access_token: &str,
    refresh_token: &str,
    expires_in_secs: i64,
) -> Result<String, String> {
    let id = format!("cal-{}", uuid_v4());
    let access_blob = secrets::protect(access_token).unwrap_or_default();
    let refresh_blob = if refresh_token.is_empty() {
        String::new()
    } else {
        secrets::protect(refresh_token).unwrap_or_default()
    };
    let account = CalendarAccount {
        id,
        provider,
        label: default_label(provider, email),
        account_email: email.to_string(),
        access_token_blob: access_blob,
        refresh_token_blob: refresh_blob,
        expires_at: now_secs() + expires_in_secs.max(1),
        poll_mins: 15,
        enabled: true,
        last_sync_at: 0,
        last_error: String::new(),
    };
    let mut accounts = load_accounts();
    accounts.push(account.clone());
    // Persist requires an app handle — the OAuth flow runs detached, so
    // grab the global handle from the tauri manager if available.
    if let Some(app) = crate::shared::app_handle() {
        persist(&app, &accounts)?;
    } else {
        // Fallback: write config directly without the event emit.
        let _ = write_accounts_direct(&accounts);
    }
    Ok(account.id)
}

/// Update an existing account in place (used by the poll thread to rotate
/// tokens + stamp `last_sync_at`). Finds by id, merges the provided
/// fields, writes back. `app` is optional — when None we skip the event.
pub fn update_account(app: Option<&tauri::AppHandle>, updated: &CalendarAccount) -> Result<(), String> {
    let mut accounts = load_accounts();
    let mut found = false;
    for acc in accounts.iter_mut() {
        if acc.id == updated.id {
            *acc = updated.clone();
            found = true;
            break;
        }
    }
    if !found {
        return Err(format!("account {} not found", updated.id));
    }
    if let Some(app) = app {
        persist(app, &accounts)?;
    } else {
        write_accounts_direct(&accounts)?;
    }
    Ok(())
}

/// Remove an account **and** every event it produced (so the calendar +
/// alarms widget don't keep stale entries). `app` optional as above.
pub fn remove_account(app: Option<&tauri::AppHandle>, account_id: &str) -> Result<(), String> {
    let accounts = load_accounts();
    let filtered: Vec<CalendarAccount> = accounts.into_iter().filter(|a| a.id != account_id).collect();
    if let Some(app) = app {
        persist(app, &filtered)?;
    } else {
        write_accounts_direct(&filtered)?;
    }
    // Delete the synced events this account authored.
    crate::events::repository::delete_by_source_account(account_id);
    Ok(())
}

/// DPAPI-unwrap the access token. Returns empty string on failure.
pub fn access_token_plain(account: &CalendarAccount) -> String {
    if account.access_token_blob.is_empty() {
        String::new()
    } else {
        secrets::unprotect(&account.access_token_blob).unwrap_or_default()
    }
}

/// DPAPI-unwrap the refresh token. Returns empty string on failure.
pub fn refresh_token_plain(account: &CalendarAccount) -> String {
    if account.refresh_token_blob.is_empty() {
        String::new()
    } else {
        secrets::unprotect(&account.refresh_token_blob).unwrap_or_default()
    }
}

/// Write the account list straight to `config.json` without emitting the
/// config-updated event. Used by detached OAuth/poll threads that lack a
/// convenient `AppHandle` (they can read the global handle, but the event
/// is best-effort — direct write guarantees durability).
fn write_accounts_direct(accounts: &[CalendarAccount]) -> Result<(), String> {
    let mut cfg = cfg_repo::load();
    let datetime = cfg.widgets.config.entry("datetime".to_string()).or_default();
    datetime.insert(
        "calendar_accounts".to_string(),
        serde_json::to_value(accounts).unwrap_or(serde_json::Value::Null),
    );
    cfg_repo::save(&cfg).map_err(|e| e.to_string())?;
    Ok(())
}

fn default_label(provider: CalendarAccountProvider, email: &str) -> String {
    if !email.is_empty() {
        return email.to_string();
    }
    match provider {
        CalendarAccountProvider::Google => "Google Calendar".into(),
        CalendarAccountProvider::Outlook => "Outlook".into(),
    }
}

/// RFC-4122 v4 UUID string.
fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Helper used by the widget-config UI: persist an account list the
/// frontend edited (e.g. toggling `enabled`, changing `label`/`poll_mins`).
/// Tokens are left untouched (the UI never re-sends them).
pub fn save_accounts(app: &tauri::AppHandle, accounts: &[CalendarAccount]) -> Result<(), String> {
    persist(app, accounts)
}

/// Build a `CalendarAccount` whose tokens are already DPAPI-wrapped
/// (used by refresh-token rotation before `update_account`).
pub fn wrap_tokens(
    account: &CalendarAccount,
    new_access: &str,
    new_access_expires_in: i64,
) -> CalendarAccount {
    let access_blob = secrets::protect(new_access).unwrap_or_default();
    let mut next = account.clone();
    next.access_token_blob = access_blob;
    next.expires_at = now_secs() + new_access_expires_in.max(1);
    next
}
