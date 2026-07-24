//! Thin Tauri command adapters for the `aicli` domain.
//!
//! Each command is a thin wrapper: receive args → call a pure service →
//! optionally emit. No business logic here (AGENTS §3). The single
//! `zenith:aicli-changed` emitter lives in `listen.rs`. 2026-07-23 rebuild

use std::os::windows::process::CommandExt as _;
use std::process::Command;
use tauri::{AppHandle, Manager};

use super::detect::resolve_bin;
use super::model::{AicliHookStatus, AicliState};

/// Current cached aggregate state. Read by the bar widget + window on open.
#[tauri::command]
pub fn get_aicli_state() -> AicliState {
    super::listen::snapshot()
}

/// Open the 340×440 agents window anchored/centered. Reuses the git-manager
/// builder pattern via `create_aicli_window`.
#[tauri::command]
pub async fn open_aicli_window(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("ai-cli") {
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    // WebviewWindowBuilder::build() blocks while it initialises the
    // WebView2 process — running it on the tauri async runtime can
    // deadlock the IPC channel (the frontend's invoke() never resolves and
    // the click appears to do nothing). Mirror git/volume and off-load to a
    // blocking thread.
    tauri::async_runtime::spawn_blocking(move || create_aicli_window(&app, x, y))
        .await
        .map_err(|e| e.to_string())?
}

/// Install managed hooks for the given CLI id. Returns nothing on success.
#[tauri::command]
pub fn aicli_install_hooks(id: String) -> Result<(), String> {
    let cli = super::listen::parse_cli(&id)?;
    super::hooks::install(cli)?;
    super::listen::poke();
    Ok(())
}

/// Remove managed hooks for the given CLI id (preserving user hooks).
#[tauri::command]
pub fn aicli_uninstall_hooks(id: String) -> Result<(), String> {
    let cli = super::listen::parse_cli(&id)?;
    super::hooks::uninstall(cli)?;
    super::listen::poke();
    Ok(())
}

/// Per-CLI managed-hook status (for the widget-config UI).
#[tauri::command]
pub fn aicli_hook_status() -> Vec<AicliHookStatus> {
    super::hooks::status()
}

/// Launch a new terminal window running the given AI CLI.
/// Tries Windows Terminal first, falls back to cmd.exe with a new console.
#[tauri::command]
pub fn start_cli(id: String) -> Result<(), String> {
    let cli = super::listen::parse_cli(&id)?;
    let cmd = cli.bin();
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into());

    // Try Windows Terminal first — native on Win11.
    if let Some(wt) = resolve_bin("wt") {
        let mut c = Command::new(wt);
        c.arg("-w").arg("-1").arg("cmd").arg("/k").arg(cmd);
        c.current_dir(&home);
        c.creation_flags(0x00000010);
        match c.spawn() {
            Ok(_) => return Ok(()),
            Err(e) => eprintln!("[start_cli] wt spawn failed: {e}"),
        }
    }

    // Fallback: cmd /K with its own console window.
    let mut c = Command::new("cmd");
    c.arg("/K").arg(cmd);
    c.current_dir(&home);
    c.creation_flags(0x00000010);
    c.spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to launch {cmd}: {e}"))
}

/// Build the 340×440 resizable agents window. Mirrors `create_git_manager`
/// (git/commands.rs): transparent + acrylic, rounded corners, the
/// §13.10b show sequence. Anchored under the bar widget when `x,y` are
/// provided, else centered.
pub fn create_aicli_window(app: &AppHandle, x: f64, y: f64) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    };

    let (wx, wy, ww, wh) = crate::window::monitor::clamp_to_monitor(
        x.round() as i32, y.round() as i32, AI_CLI_W as i32, AI_CLI_H as i32,
    );

    let positioned = x > 0.0 && y > 0.0;
    let mut builder = tauri::WebviewWindowBuilder::new(
        app,
        "ai-cli",
        tauri::WebviewUrl::App("widgets/ai_cli/window/ai-cli.html".into()),
    )
    .title("AI Agents")
    .inner_size(ww as f64, wh as f64)
    .min_inner_size(300.0, 360.0)
    .max_inner_size(600.0, 700.0)
    .resizable(true)
    .decorations(false)
    .transparent(true)
    .skip_taskbar(false)
    .focused(true)
    .additional_browser_args("--default-background-color=00000000");

    if positioned {
        builder = builder.position(wx as f64, wy as f64);
    } else {
        builder = builder.center();
    }
    // Start hidden; revealed after the acrylic material is registered.
    builder = builder.visible(false);

    let win = builder.build().map_err(|e| e.to_string())?;

    let _ = crate::window::apply_fixed_acrylic(app, "ai-cli");
    let _ = crate::window::set_rounded_corners(&win);
    let _ = crate::window::set_disable_transitions(&win);

    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOSIZE | SWP_NOMOVE,
        )
    };
    let _ = win.set_focus();

    Ok(())
}

const AI_CLI_W: f64 = 460.0;
const AI_CLI_H: f64 = 440.0;
