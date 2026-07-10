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

use tauri::{Emitter, Manager};

use super::listen;
use super::model::{GitState, GitWidgetConfig};
use super::secrets;
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

    // Reuse an already-open window: unminimize, show, center, bring to front.
    if let Some(win) = app.get_webview_window(GIT_MANAGER_LABEL) {
        let _ = win.unminimize();
        let _ = win.set_size(tauri::LogicalSize::new(GIT_MANAGER_W as f64, GIT_MANAGER_H as f64));
        let _ = win.center();
        let _ = win.show();
        bring_to_front(&win);
        let _ = win.set_focus();
        let _ = win.emit(crate::shared::EVENT_GIT_CHANGED, listen::snapshot());
        return Ok(());
    }

    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || create_git_manager(&app_clone, x, y))
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
    let mut cmd = std::process::Command::new(&bin);
    cmd.args(&args);
    // CREATE_NEW_CONSOLE (0x10): give the assistant its own window instead of
    // inheriting the hidden Zenith console.
    cmd.creation_flags(0x00000010);
    cmd.spawn()
        .map(|_| true)
        .map_err(|e| format!("Failed to launch {bin}: {e}"))
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
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb = HSTRING::from("open");
    let file = HSTRING::from(url.as_str());
    let r = unsafe {
        ShellExecuteW(None, &verb, &file, None, None, SW_SHOWNORMAL)
    };
    // HINSTANCE > 32 (casted as a usize) means success per Win32 convention.
    r.0 as usize > 32
}

#[tauri::command]
pub fn get_git_widget_config() -> GitWidgetConfig {
    let cfg = crate::config::repository::load();
    let raw = serde_json::to_value(&cfg).unwrap_or(serde_json::Value::Null);
    raw.pointer("/widgets/config/git")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
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
        tauri::WebviewUrl::App("git-manager.html".into()),
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

    // Show after material is registered, then bring to front. We deliberately
    // drop SWP_NOZORDER so HWND_TOP actually takes effect (§13.10b's
    // SWP_NOZORDER keeps the just-created invisible window behind everything,
    // which is the "opens behind" bug). set_focus() is the foreground safety net.
    bring_to_front(&win);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = win.set_focus();

    Ok(())
}

/// Reveal a window at the top of the z-order (above other normal windows,
/// not permanently topmost). Uses `HWND_TOP` as the insert-after handle and
/// intentionally omits `SWP_NOZORDER` so that placement is honored — passing
/// `None` + `SWP_NOZORDER` silently keeps a freshly-created invisible window
/// behind everything.
fn bring_to_front(win: &tauri::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_TOP, SetWindowPos, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };
    if let Ok(hwnd) = win.hwnd() {
        let _ = unsafe {
            SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_SHOWWINDOW | SWP_NOSIZE | SWP_NOMOVE)
        };
    }
}
