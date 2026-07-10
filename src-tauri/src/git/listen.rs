//! Background poll thread for the git manager widget.
//!
//! Sequential per-account HTTPS fan-out on a single worker thread.
//! `Mutex<GitState>` cache (read by `get_git_state`); emits
//! `zenith:git-changed` with the snapshot only when the totals actually
//! change (mirrors `media::listen`).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use super::model::{AcctInventory, GitAccount, GitState};
use super::provider::inventory_for;
use super::secrets;
use crate::config::repository as cfg_repo;

pub const EVENT_GIT_CHANGED: &str = "zenith:git-changed";

macro_rules! glog {
    ($($arg:tt)*) => {{ eprintln!("[git:poll] {}", format_args!($($arg)*)); }};
}

static STATE: Mutex<Option<GitState>> = Mutex::new(None);
static LAST_KEY: Mutex<String> = Mutex::new(String::new());
static RUNNING: AtomicBool = AtomicBool::new(false);
static FORCE: AtomicBool = AtomicBool::new(false);

/// Public read for commands::get_git_state.
pub fn snapshot() -> GitState {
    STATE
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// Trigger one immediate forced cycle now (used by commands::git_refresh).
/// Sets the `FORCE` flag so the next `poll_once` bypasses the per-account
/// `poll_mins` cache and re-fetches every enabled account.
pub fn poke() {
    FORCE.store(true, Ordering::SeqCst);
    glog!("refresh poke (forced)");
}

pub fn spawn(app: AppHandle) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        glog!("thread start");
        // Immediate first poll so the bar gets populated right away
        // instead of waiting 30s for the first cycle.
        poll_once(&app);
        loop {
            std::thread::sleep(Duration::from_secs(30));
            if app.get_webview_window("bar").is_none()
                && app.get_webview_window("git-manager").is_none()
            {
                continue;
            }
            poll_once(&app);
        }
    });
}

fn poll_once(app: &AppHandle) {
    let forced = FORCE
        .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
        .unwrap_or(false);

    let cfg = cfg_repo::load();
    let git_cfg = load_git_widget_config(&cfg);
    let accounts: Vec<GitAccount> = git_cfg
        .accounts
        .clone()
        .into_iter()
        .filter(|a| a.enabled)
        .collect();
    if accounts.is_empty() {
        if let Ok(mut g) = STATE.lock() {
            *g = Some(GitState::default());
        }
        let _ = app.emit(EVENT_GIT_CHANGED, &GitState::default());
        return;
    }

    let mut state = GitState::default();
    for acct in &accounts {
        let prev_sync_ms = read_prev_sync_ms(&acct.id);
        let poll_ms = (acct.poll_mins.max(1) * 60_000) as i64;
        let now_ms = now_ms();
        if !forced && prev_sync_ms > 0 && (now_ms - prev_sync_ms) < poll_ms {
            if let Some(inv) = lookup_inventory(&acct.id) {
                state.inventories.push(inv);
                go_total(&mut state);
                continue;
            }
        }

        let token = if acct.token_blob.is_empty() {
            String::new()
        } else {
            secrets::unprotect(&acct.token_blob).unwrap_or_default()
        };
        let inv = if token.is_empty() {
            let mut inv = AcctInventory::empty(acct);
            inv.last_error = "missing token".into();
            inv
        } else {
            match inventory_for(acct, &token) {
                Ok(inv) => inv,
                Err(e) => {
                    glog!("inventory {} ({}) err: {e}", acct.label, acct.provider);
                    let mut inv = AcctInventory::empty(acct);
                    inv.last_error = e;
                    inv
                }
            }
        };
        glog!(
            "inventory {}: {} failed, {} PRs ({}ms ago) [{}]",
            acct.label,
            inv.failed_runs.len(),
            inv.open_pulls.len(),
            if inv.last_sync_ms > 0 { (now_ms - inv.last_sync_ms) / 1000 } else { 0 },
            inv.last_error,
        );
        state.inventories.push(inv);
    }
    go_total(&mut state);

    let key = format!(
        "{}|{}|{}/{}",
        state.total_failed,
        state.total_open_prs,
        state.inventories.len(),
        state.inventories.iter().map(|i| i.repos.len() as u32).sum::<u32>(),
    );
    let changed = match LAST_KEY.lock() {
        Ok(mut last) => {
            if last.as_str() == key.as_str() {
                false
            } else {
                last.clear();
                last.push_str(&key);
                true
            }
        }
        Err(_) => false,
    };

    if let Ok(mut g) = STATE.lock() {
        *g = Some(state.clone());
    }

    if changed {
        glog!(
            "changed emit: total_failed={} total_prs={}",
            state.total_failed,
            state.total_open_prs,
        );
        let _ = app.emit(EVENT_GIT_CHANGED, &state);
    }
}

fn go_total(state: &mut GitState) {
    state.total_failed = state.inventories.iter().map(|i| i.failed_runs.len() as u32).sum();
    state.total_open_prs =
        state.inventories.iter().map(|i| i.open_pulls.len() as u32).sum();
}

fn read_prev_sync_ms(acct_id: &str) -> i64 {
    STATE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| s.clone()))
        .and_then(|s| {
            s.inventories
                .into_iter()
                .find(|i| i.account_id == acct_id)
                .map(|i| i.last_sync_ms)
        })
        .unwrap_or(0)
}

fn lookup_inventory(acct_id: &str) -> Option<AcctInventory> {
    STATE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| s.clone()))
        .and_then(|s| s.inventories.into_iter().find(|i| i.account_id == acct_id))
}

fn load_git_widget_config(cfg: &crate::config::model::Config) -> super::model::GitWidgetConfig {
    // Read via JSON pointer to avoid leaking git-config through a typed
    // accessor on Config (keeps the config aggregate lean — git is just a
    // widget config payload, not a top-level domain).
    let raw = serde_json::to_value(cfg).unwrap_or(serde_json::Value::Null);
    raw.pointer("/widgets/config/git")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
