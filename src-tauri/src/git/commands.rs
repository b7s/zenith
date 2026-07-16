//! Thin `#[tauri::command]` adapters for the git widget domain.
//!
//! Commands:
//!   - `open_git_manager(x, y)` — opens the manager window anchored under
//!     the bar widget that triggered it. Mirrors the calendar popup
//!     creation flow (transparent, frameless, acrylic, monitor-clamped,
//!     visibility-after-material sequence per §13.10a/§13.10b).
//!   - `get_git_state()` — returns the cached snapshot (filtered by
//!     selected account id when supplied). Cheap; never makes HTTP calls.
//!   - `git_refresh()` — pokes the poll thread so the next cycle is now.
//!   - `protect_secret(plaintext)` — DPAPI-wrap a token. Used by the
//!     widget-config window when saving accounts.

use std::os::windows::process::CommandExt as _;
use std::sync::Mutex;

use tauri::Manager;

use super::listen;
use super::model::{GitState, GitWidgetConfig};
use super::secrets;
use serde_json::Value;
use crate::window;

const GIT_MANAGER_LABEL: &str = "git-manager";
const GIT_MANAGER_W: i32 = 760;
const GIT_MANAGER_H: i32 = 540;

/// Selected-account id passed to the window via `__ZENITH_GIT_ACCOUNT_ID`
/// init script (nul = "All"). Mirrors the dialog/calendar init-script
/// pattern.
static SELECTED_ACCT: Mutex<Option<String>> = Mutex::new(None);

#[tauri::command]
pub async fn open_git_manager(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
    account_id: Option<String>,
) -> Result<(), String> {
    if let Ok(mut g) = SELECTED_ACCT.lock() {
        *g = account_id.clone();
    }

    // Toggle: if the manager is already open, close it (clicking the bar
    // widget again dismisses it). Account selection is honored on next open.
    if let Some(win) = app.get_webview_window(GIT_MANAGER_LABEL) {
        let _ = win.close();
        return Ok(());
    }

    // Window creation must run on the main thread — WebviewWindowBuilder::build()
    // creates an HWND that needs a message pump on its creator thread.
    // spawn_blocking doesn't pump messages, so the window's SetWindowPos /
    // SetWindowCompositionAttribute messages never get processed and the
    // window stays invisible. We post the work to the main thread via
    // run_on_main_thread, then wait for the result via a channel.
    let (tx, rx) = std::sync::mpsc::channel();
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(create_git_manager(&app2, x, y));
    })
    .map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|_| "channel closed".to_string())?)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn get_git_state(account_id: Option<String>) -> GitState {
    let mut state = listen::snapshot();
    if let Some(id) = account_id {
        if !id.is_empty() {
            state.inventories.retain(|i| i.account_id == id);
            state.total_failed =
                state.inventories.iter().map(|i| i.failed_runs.len() as u32).sum();
            state.total_open_prs =
                state.inventories.iter().map(|i| i.open_pulls.len() as u32).sum();
        }
    }
    state
}

#[tauri::command]
pub fn git_refresh() -> bool {
    listen::poke();
    true
}

#[tauri::command]
pub fn get_git_selected_account() -> Option<String> {
    SELECTED_ACCT.lock().ok().and_then(|g| g.clone())
}

#[tauri::command]
pub fn protect_secret(plaintext: String) -> Result<String, String> {
    secrets::protect(&plaintext).ok_or_else(|| "DPAPI protect failed — your Windows profile may not be loaded".into())
}

#[tauri::command]
pub fn unprotect_secret_for_selftest() -> bool {
    // Sanity check exposed to the widget-config window: returns true
    // only if DPAPI protect+unprotect works in this process. The window
    // uses it to disable the "Add account" button when DPAPI is
    // unavailable (corporate service accounts can fail this).
    secrets::protect("zenith-selftest")
        .and_then(|b| secrets::unprotect(&b))
        .map(|s| s == "zenith-selftest")
        .unwrap_or(false)
}

/// Read the saved git widget config (accounts etc.) so the
/// frontend can render the account selector pills without doing
/// JSON-pointer walking itself.
/// Open an external URL in the user's default browser via `ShellExecuteW`.
/// Used by the manager window cards so users can jump straight to a failed
/// run or PR on the provider's site. Returns true on success.
/// Launch an AI CLI in a fresh console window with a prefilled analysis
/// prompt for a failed CI run or an open PR. `cli` selects the assistant
/// (must be one of the ids the user enabled in the git widget config); the
/// prompt already contains the failure/PR context + the git identifier.
#[tauri::command]
pub fn send_to_ai(cli: String, prompt: String) -> Result<bool, String> {
    let (bin, args) = cli_invocation(&cli, &prompt)
        .ok_or_else(|| format!("Unknown AI assistant: {cli}"))?;

    // Resolve the binary up front. Windows hides the spawned console the
    // instant the child exits, so for a missing CLI we'd get a console that
    // flashes an error and vanishes before the user can read it. Detect the
    // missing-binary case here and return a clean error instead of spawning
    // anything — the frontend shows a dialog, no flashing window.
    let resolved = resolve_bin(&bin).ok_or_else(|| {
        format!(
            "The '{bin}' CLI is not installed or not on your PATH. \
             Install it and restart Zenith to use this assistant."
        )
    })?;

    eprintln!("[send_to_ai] launching {resolved:?} with args: {:?}", args);

    let is_script = matches!(
        resolved
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_uppercase())
            .as_deref(),
        Some("CMD") | Some("BAT") | Some("COM")
    );

    if is_script {
        // npm-installed AI CLIs are `.cmd`/`.bat` shims. Windows refuses to
        // spawn them directly with arguments ("batch file arguments are
        // invalid"), and a bare `cmd /C` makes the console vanish the instant
        // the shim's child exits. Prefer the modern Windows Terminal (wt.exe)
        // so the assistant runs inside a persistent, hosted tab; fall back to
        // `cmd /K` (which also keeps the console open) when wt.exe is missing.
        let path_env = std::env::var("PATH").unwrap_or_default();
        if let Some(wt) = resolve_bin("wt") {
            let mut cmd = std::process::Command::new(wt);
            // `-w -1` opens a new Terminal window; `cmd /k "<shim> <args>"`
            // keeps the tab alive after the assistant exits.
            cmd.arg("-w").arg("-1").arg("cmd").arg("/k").arg(&resolved);
            cmd.args(&args);
            cmd.creation_flags(0x00000010);
            cmd.env("PATH", &path_env);
            return cmd.spawn().map(|_| true).map_err(|e| {
                eprintln!("[send_to_ai] wt spawn failed: {e}");
                format!("{bin} failed to start: {e}")
            });
        }
        let mut cmd = std::process::Command::new("cmd");
        cmd.arg("/K").arg(&resolved).args(&args);
        // CREATE_NEW_CONSOLE (0x10): give the assistant its own window.
        cmd.creation_flags(0x00000010);
        cmd.env("PATH", path_env);
        return cmd.spawn().map(|_| true).map_err(|e| {
            eprintln!("[send_to_ai] script spawn failed: {e}");
            format!("{bin} failed to start: {e}")
        });
    }

    // Direct spawn for real executables (.exe). CREATE_NEW_CONSOLE (0x10)
    // gives the assistant its own window instead of the hidden Zenith one.
    let mut cmd = std::process::Command::new(&resolved);
    cmd.args(&args);
    cmd.creation_flags(0x00000010);
    // Ensure we inherit the parent's PATH so Windows can find sibling tools.
    cmd.env("PATH", std::env::var("PATH").unwrap_or_default());
    match cmd.spawn() {
        Ok(_) => Ok(true),
        Err(direct_err) => {
            eprintln!("[send_to_ai] direct spawn failed: {direct_err}; retrying via cmd /C");
            // Direct spawn failed for a non-script binary — fall back to
            // delegating to `cmd.exe /C`, which performs the full PATHEXT
            // search and launches the right binary in a new console. We pass
            // the resolved path so cmd can't report "not found" (we already
            // verified it exists above).
            let mut fallback = std::process::Command::new("cmd");
            fallback.arg("/C").arg(&resolved).args(&args);
            fallback.creation_flags(0x00000010);
            fallback.env("PATH", std::env::var("PATH").unwrap_or_default());
            fallback.spawn().map(|_| true).map_err(|e| {
                eprintln!("[send_to_ai] cmd fallback also failed: {e}");
                format!("{bin} failed to start: {e}")
            })
        }
    }
}

/// Resolve an executable name against the process PATH, honouring the
/// Windows `PATHEXT` list so `foo`, `foo.exe`, `foo.cmd`, `foo.bat`, etc.
/// all resolve. Returns `None` when nothing matches (the CLI isn't
/// installed), so callers can surface a friendly error instead of spawning
/// a console that closes instantly on a missing binary.
fn resolve_bin(bin: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into());
    let exts: Vec<String> = pathext
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_uppercase())
        .collect();
    let has_ext = std::path::Path::new(bin)
        .extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .map(|e| exts.iter().any(|x| x == &e))
        .unwrap_or(false);

    let candidates: Vec<String> = if has_ext {
        vec![bin.to_string()]
    } else {
        exts.iter().map(|e| format!("{bin}{e}")).collect()
    };

    for dir in std::env::split_paths(&path) {
        for cand in &candidates {
            let full = dir.join(cand);
            if full.is_file() {
                return Some(full);
            }
        }
    }
    None
}

/// Map an AI assistant id to its binary + argument list. The prompt is
/// always passed as a discrete argument so the OS quotes it safely.
fn cli_invocation(cli: &str, prompt: &str) -> Option<(String, Vec<String>)> {
    let p = prompt.to_string();
    let spec: (&str, Vec<String>) = match cli {
        "opencode" => ("opencode", vec!["-p".into(), p]),
        "codex" => ("codex", vec![p]),
        "claude" => ("claude", vec!["-p".into(), p]),
        "cursor" => ("cursor", vec![p]),
        "gemini" => ("gemini", vec!["-p".into(), p]),
        "amp" => ("amp", vec!["-m".into(), p]),
        "soloterm" => ("soloterm", vec![p]),
        _ => return None,
    };
    Some((spec.0.to_string(), spec.1))
}

#[tauri::command]
pub fn open_url(url: String) -> bool {
    crate::shared::shell::open_url(&url)
}

#[tauri::command]
pub fn get_git_widget_config() -> GitWidgetConfig {
    let cfg = crate::config::repository::load();
    let raw = serde_json::to_value(&cfg).unwrap_or(serde_json::Value::Null);
    raw.pointer("/widgets/config/git")
        .and_then(|v| {
            serde_json::from_value(v.clone())
                .inspect_err(|e| eprintln!("[git] parse widget config: {e}"))
                .ok()
        })
        .unwrap_or_default()
}

/// Lazily fetch the real content for a card so the user can copy it without
/// Zenith preloading (and holding in memory) the full PR body / CI log for
/// every card. Called only when the user clicks "Copy content".
///
/// - `run`: the genuine failure summary is already captured during the poll
///   cycle into `FailRun.error` (truncated check-run output). Return it
///   directly — no network call, instant and RAM-free.
/// - `pr`: fetch the PR/MR description (+ diff) on demand from the provider
///   REST API using the stored account token.
#[tauri::command]
pub fn fetch_git_content(
    kind: String,
    account_id: String,
    full_name: String,
    number: Option<u64>,
    cached_error: String,
) -> Result<String, String> {
    if kind == "run" {
        if cached_error.trim().is_empty() {
            return Err("No failure log was captured for this run.".into());
        }
        return Ok(cached_error);
    }

    // PR: resolve the account (token + provider + host) and fetch on demand.
    let cfg = get_git_widget_config();
    let acct = cfg
        .accounts
        .iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| "Account not found for this item.".to_string())?;
    let token = secrets::unprotect(&acct.token_blob)
        .ok_or_else(|| "Could not decrypt the account token.".to_string())?;

    match acct.provider.as_str() {
        "github" => fetch_github_pr(&full_name, number.unwrap_or(0), &token),
        "gitlab" => fetch_gitlab_pr(&acct.host_url, &full_name, number.unwrap_or(0), &token),
        "forgejo" | "gitea" => {
            fetch_forgejo_pr(&acct.host_url, &full_name, number.unwrap_or(0), &token)
        }
        other => Err(format!("Copy content is not supported for '{other}' yet.")),
    }
}

fn fetch_github_pr(full_name: &str, number: u64, token: &str) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{full_name}/pulls/{number}");
    let v = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .set("Authorization", &format!("Bearer {token}"))
        .set("User-Agent", "zenith")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("github: {e}"))?
        .into_json::<Value>()
        .map_err(|e| format!("github read: {e}"))?;
    let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let body = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();

    let diff = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .set("Authorization", &format!("Bearer {token}"))
        .set("User-Agent", "zenith")
        .set("Accept", "application/vnd.github.v3.diff")
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
        .unwrap_or_default();

    let mut out = format!("PR #{number}: {title}\n\n");
    out.push_str(&body);
    if !diff.is_empty() {
        out.push_str("\n\n--- DIFF ---\n");
        out.push_str(&diff);
    }
    Ok(out)
}

fn fetch_gitlab_pr(host_url: &str, full_name: &str, number: u64, token: &str) -> Result<String, String> {
    let base = if host_url.is_empty() { "https://gitlab.com" } else { host_url };
    let proj = full_name.replace('/', "%2F");
    let url = format!("{base}/api/v4/projects/{proj}/merge_requests/{number}");
    let v = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .set("PRIVATE-TOKEN", token)
        .set("User-Agent", "zenith")
        .call()
        .map_err(|e| format!("gitlab: {e}"))?
        .into_json::<Value>()
        .map_err(|e| format!("gitlab read: {e}"))?;
    let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let desc = v.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string();

    let changes_url = format!("{base}/api/v4/projects/{proj}/merge_requests/{number}/changes");
    let diff = ureq::get(&changes_url)
        .timeout(std::time::Duration::from_secs(15))
        .set("PRIVATE-TOKEN", token)
        .set("User-Agent", "zenith")
        .call()
        .ok()
        .and_then(|r| r.into_json::<Value>().ok())
        .and_then(|c| c.get("changes").and_then(|a| a.as_array()).cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("diff").and_then(|d| d.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();

    let mut out = format!("MR !{number}: {title}\n\n");
    out.push_str(&desc);
    if !diff.is_empty() {
        out.push_str("\n\n--- DIFF ---\n");
        out.push_str(&diff);
    }
    Ok(out)
}

fn fetch_forgejo_pr(host_url: &str, full_name: &str, number: u64, token: &str) -> Result<String, String> {
    let base = if host_url.is_empty() { "https://codeberg.org" } else { host_url };
    let url = format!("{base}/api/v1/repos/{full_name}/pulls/{number}");
    let v = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .set("Authorization", &format!("Bearer {token}"))
        .set("User-Agent", "zenith")
        .call()
        .map_err(|e| format!("forgejo: {e}"))?
        .into_json::<Value>()
        .map_err(|e| format!("forgejo read: {e}"))?;
    let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let body = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();

    let diff_url = format!("{base}/api/v1/repos/{full_name}/pulls/{number}.diff");
    let diff = ureq::get(&diff_url)
        .timeout(std::time::Duration::from_secs(15))
        .set("Authorization", &format!("Bearer {token}"))
        .set("User-Agent", "zenith")
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
        .unwrap_or_default();

    let mut out = format!("PR #{number}: {title}\n\n");
    out.push_str(&body);
    if !diff.is_empty() {
        out.push_str("\n\n--- DIFF ---\n");
        out.push_str(&diff);
    }
    Ok(out)
}

fn create_git_manager(app: &tauri::AppHandle, _x: f64, _y: f64) -> Result<(), String> {
    let acct_id = SELECTED_ACCT.lock().ok().and_then(|g| g.clone());
    let init_script = format!(
        "window.__ZENITH_GIT_ACCOUNT_ID = {};",
        match acct_id {
            Some(s) => format!("\"{}\"", s.replace('"', "\\\"")),
            None => "null".to_string(),
        }
    );

    let win = tauri::WebviewWindowBuilder::new(
        app,
        GIT_MANAGER_LABEL,
        tauri::WebviewUrl::App("widgets/git/window/git-manager.html".into()),
    )
    .title("Git Manager")
    .inner_size(GIT_MANAGER_W as f64, GIT_MANAGER_H as f64)
    .min_inner_size(560.0, 380.0)
    .max_inner_size(1200.0, 800.0)
    .resizable(true)
    .decorations(false)
    .transparent(true)
    .skip_taskbar(false)
    .visible(false)
    .focused(true)
    .center()
    .additional_browser_args("--default-background-color=00000000")
    .initialization_script(&init_script)
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window::apply_fixed_acrylic(app, GIT_MANAGER_LABEL);
    let _ = window::set_rounded_corners(&win);
    let _ = window::set_disable_transitions(&win);

    // Show after material is registered, then bring to front using the same
    // pattern as the Settings/Widgets windows (§13.10b): reveal with
    // SWP_SHOWWINDOW + SWP_NOZORDER + SWP_NOSIZE + SWP_NOMOVE, then set_focus.
    // This avoids the custom HWND_TOP bring_to_front path that left the window
    // minimized/hidden on open.
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_SHOWWINDOW, SWP_NOZORDER, SWP_NOSIZE, SWP_NOMOVE};
    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    let _ = unsafe { SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOSIZE | SWP_NOMOVE) };
    let _ = win.set_focus();

    Ok(())
}
