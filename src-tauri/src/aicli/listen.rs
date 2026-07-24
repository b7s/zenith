//! Background poll loop for the AI-agent status widget.
//!
//! Every `POLL_MS` we rebuild the `AicliState`: for each monitored CLI, take
//! the transcript/server-derived session from `scan` and overlay any recent
//! hook events (precise `waiting`/`failed`). The aggregate is cached; the
//! `zenith:aicli-changed` event fires only when the diff key changes
//! (mirrors `git::listen` + `workspace::notification` de-dupe philosophy).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use super::model::{
    AicliState, CliId, CliSession, CliStatus,
};
use super::server::{self, HookEvent};
use crate::config::repository as cfg_repo;
use crate::shared::EVENT_AICLI_CHANGED;

const POLL_MS: u64 = 3_000;
static FORCE: AtomicBool = AtomicBool::new(false);

static STATE: Mutex<Option<AicliState>> = Mutex::new(None);

/// Public read for `get_aicli_state`.
pub fn snapshot() -> AicliState {
    STATE
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// Parse a CLI id string into a `CliId`.
pub fn parse_cli(id: &str) -> Result<CliId, String> {
    match id {
        "claude" => Ok(CliId::Claude),
        "codex" => Ok(CliId::Codex),
        "opencode" => Ok(CliId::Opencode),
        _ => Err(format!("unknown CLI id: {id}")),
    }
}

/// Read the widget config's monitoring set. First-run seeding: when no config
/// key exists yet, monitor **all installed** CLIs (safe-getter pattern, §5).
fn monitored_ids() -> Vec<CliId> {
    let cfg = cfg_repo::load();
    let raw = serde_json::to_value(&cfg).unwrap_or(serde_json::Value::Null);
    let wc = raw
        .pointer("/widgets/config/ai_cli")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let keys = ["monitor_claude", "monitor_codex", "monitor_opencode"];
    let ids = [CliId::Claude, CliId::Codex, CliId::Opencode];
    let mut out = Vec::new();
    let mut any_present = false;
    for (k, id) in keys.iter().zip(ids.iter()) {
        if let Some(v) = wc.get(*k) {
            any_present = true;
            if v.as_bool().unwrap_or(false) {
                out.push(*id);
            }
        }
    }
    if !any_present {
        // First run (or config cleared): monitor everything installed.
        out = super::detect::installed_ids();
    }
    out
}

/// Force an immediate re-poll (called after hook install/uninstall).
pub fn poke() {
    FORCE.store(true, Ordering::SeqCst);
}

fn apply_hook_events(sessions: &mut [CliSession], events: &[HookEvent], now: i64) {
    for ev in events {
        let Some(slot) = sessions.iter_mut().find(|s| s.id == ev.source.as_str()) else {
            continue;
        };
        slot.running = true;
        slot.updated_ms = now;
        let ev_str = ev.event.as_str();
        match ev.source {
            CliId::Claude => match ev_str {
                "UserPromptSubmit" | "SessionStart" => {
                    if let Some(t) = first_user_prompt(&ev.payload) {
                        slot.title = t;
                    }
                    slot.status = CliStatus::Running;
                }
                "Notification" => {
                    let sub = ev
                        .payload
                        .get("subtype")
                        .and_then(|s| s.as_str())
                        .unwrap_or("");
                    slot.status = if sub.contains("permission") {
                        CliStatus::Waiting
                    } else if sub.contains("idle") {
                        CliStatus::Idle
                    } else {
                        CliStatus::Running
                    };
                }
                "StopFailure" => slot.status = CliStatus::Failed,
                "Stop" => {
                    if slot.status != CliStatus::Failed {
                        slot.status = CliStatus::Idle;
                    }
                }
                _ => {}
            },
            CliId::Codex => match ev_str {
                "session_start" | "user_prompt_submit" => slot.status = CliStatus::Running,
                "permission" | "waiting_for_permission" => slot.status = CliStatus::Waiting,
                "session_end" | "stop" => {
                    if slot.status != CliStatus::Failed {
                        slot.status = CliStatus::Idle;
                    }
                }
                _ => {}
            },
            CliId::Opencode => match ev_str {
                "session_start" => slot.status = CliStatus::Running,
                "session_end" => {
                    if slot.status != CliStatus::Failed {
                        slot.status = CliStatus::Idle;
                    }
                }
                _ => {}
            },
        }
    }
}

fn first_user_prompt(v: &serde_json::Value) -> Option<String> {
    v.get("prompt")
        .and_then(|p| p.as_str())
        .map(|s| s.chars().take(64).collect())
        .or_else(|| {
            v.get("payload")
                .and_then(|p| p.get("prompt"))
                .and_then(|p| p.as_str())
                .map(|s| s.chars().take(64).collect())
        })
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn poll_once() -> Option<AicliState> {
    let monitored = monitored_ids();
    let installed = super::detect::installed_ids();
    let now = now_ms();

    let mut sessions: Vec<CliSession> = monitored
        .iter()
        .flat_map(|id| super::scan::sessions_for(*id, installed.contains(id)))
        .collect();

    // Overlay hook events for precise waiting/failed.
    let events = server::drain_events();
    if !events.is_empty() {
        apply_hook_events(&mut sessions, &events, now);
    }

    let mut state = AicliState {
        sessions,
        totals: Default::default(),
        monitored: monitored.iter().map(|i| i.as_str().to_string()).collect(),
    };
    state.recompute_totals();

    Some(state)
}

/// Emit only when the diff key changes.
fn maybe_emit(app: &AppHandle, state: &AicliState) {
    let key = diff_key(state);
    let changed = {
        let mut g = match STATE.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        match &mut *g {
            None => {
                *g = Some(state.clone());
                true
            }
            Some(prev) => {
                let prev_key = diff_key(prev);
                if prev_key == key {
                    false
                } else {
                    *prev = state.clone();
                    true
                }
            }
        }
    };
    if changed {
        let _ = app.emit(EVENT_AICLI_CHANGED, state);
    }
}

/// A compact signature so we only emit on real changes (not every 3s tick).
/// Includes `cwd` + `updated_ms` + title-length so two concurrent sessions
/// from the same CLI (e.g. two OpenCode sessions in different repos) hash
/// distinctly and the second one's appearance triggers an emit.
fn diff_key(s: &AicliState) -> String {
    let mut parts = Vec::new();
    parts.push(format!("t:{}/{}/{}", s.totals.running, s.totals.waiting, s.totals.failed));
    for sess in &s.sessions {
        parts.push(format!(
            "{}:{}:{}:{}:{}:{}",
            sess.id,
            if sess.installed { 1 } else { 0 },
            match sess.status {
                CliStatus::Running => "r",
                CliStatus::Waiting => "w",
                CliStatus::Failed => "f",
                CliStatus::Idle => "i",
            },
            sess.title.len(),
            sess.cwd,
            sess.updated_ms
        ));
    }
    parts.join("|")
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        // Immediate first poll so the bar populates right away.
        if let Some(state) = poll_once() {
            maybe_emit(&app, &state);
        }
        loop {
            std::thread::sleep(Duration::from_millis(POLL_MS));
            if app.get_webview_window("bar").is_none()
                && app.get_webview_window("ai-cli").is_none()
            {
                continue;
            }
            if let Some(state) = poll_once() {
                maybe_emit(&app, &state);
            }
            // Drain any events even if both windows closed, so the cache stays
            // fresh for the next open.
            server::drain_events();
        }
    });
}
