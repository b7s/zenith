//! Thin `#[tauri::command]` adapters for the ai-cli widget domain.

use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager};

use super::aggregator::Aggregator;
use super::detect;
use super::hook_install;
use super::model::{AggregateState, CliDetected, CliId};

use std::sync::OnceLock;

static AGGREGATOR: OnceLock<Arc<Mutex<Aggregator>>> = OnceLock::new();

pub fn aggregator() -> Arc<Mutex<Aggregator>> {
    AGGREGATOR.get_or_init(|| Arc::new(Mutex::new(Aggregator::default()))).clone()
}

const AI_CLI_MANAGER_LABEL: &str = "ai-cli-manager";
const AI_MANAGER_W: i32 = 422;
const AI_MANAGER_H: i32 = 480;

#[tauri::command]
pub fn get_ai_cli_state() -> AggregateState {
    aggregator().lock().map(|a| a.aggregate()).unwrap_or_default()
}

#[tauri::command]
pub fn detect_ai_clis() -> Vec<CliDetected> {
    detect::detect_all()
}

#[tauri::command]
pub async fn install_ai_cli_hooks(_app: tauri::AppHandle, clis: Vec<String>) -> Result<bool, String> {
    for cli_str in &clis {
        if let Some(cli) = CliId::parse(cli_str) {
            hook_install::install(cli)?;
        }
    }
    Ok(true)
}

#[tauri::command]
pub async fn uninstall_ai_cli_hooks(_app: tauri::AppHandle, clis: Vec<String>) -> Result<bool, String> {
    for cli_str in &clis {
        if let Some(cli) = CliId::parse(cli_str) {
            hook_install::uninstall(cli)?;
        }
    }
    Ok(true)
}

#[tauri::command]
pub async fn ack_ai_cli_failures() -> bool {
    if let Ok(mut a) = aggregator().lock() {
        a.ack_all();
    }
    // Broadcast the updated state so the bar widget re-paints
    if let Some(h) = crate::shared::app_handle() {
        let state = get_ai_cli_state();
        let _ = h.emit(crate::shared::EVENT_AI_CLI_CHANGED, &state);
    }
    true
}

#[tauri::command]
pub async fn open_ai_cli_manager(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
) -> Result<(), String> {
    // Toggle close if already open
    if let Some(win) = app.get_webview_window(AI_CLI_MANAGER_LABEL) {
        let _ = win.close();
        return Ok(());
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let app2 = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(create_ai_cli_manager(&app2, x, y));
    })
    .map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn_blocking(move || rx.recv().map_err(|_| "channel closed".to_string())?)
        .await
        .map_err(|e| e.to_string())?
}

fn create_ai_cli_manager(app: &tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    let label = AI_CLI_MANAGER_LABEL;
    let url = "widgets/ai-cli/window/ai-manager.html";

    let win = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App(url.into()))
        .title("AI CLI Manager")
        .inner_size(AI_MANAGER_W as f64, AI_MANAGER_H as f64)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .visible(false)
        .additional_browser_args("--default-background-color=00000000")
        .build()
        .map_err(|e| e.to_string())?;

    // Monitor-clamp the proposed position (anchor from bar widget)
    let (clamped_x, clamped_y, _w, _h) = crate::window::clamp_to_monitor(x as i32, y as i32, AI_MANAGER_W, AI_MANAGER_H);
    let _ = win.set_position(tauri::PhysicalPosition::new(clamped_x as f64, clamped_y as f64));

    // Material after build, then show per §13.10a/§13.10b
    let _ = crate::window::apply_fixed_acrylic(app, "ai-cli-manager");
    let _ = crate::window::set_rounded_corners(&win);

    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    let _ = unsafe {
        windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            windows::Win32::UI::WindowsAndMessaging::SWP_SHOWWINDOW
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE,
        )
    };
    let _ = win.set_focus();

    Ok(())
}
