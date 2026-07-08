use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::Emitter;
use tauri::Manager;
use serde::Serialize;

use crate::volume;
use crate::window;
use crate::workspace;

static WS_CONTEXT_ID: AtomicU32 = AtomicU32::new(0);

/// Guard: prevents double-opening of dialogs when `on_menu_event` fires twice
/// for a single menu click (observed on Tauri 2). Cleared after creation.
static DIALOG_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Unified dialog state passed to the dialog window via `get_dialog_data`.
/// `kind` selects the body builder; `data` is opaque JSON for that builder.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct DialogSpec {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub data: Option<serde_json::Value>,
}

static DIALOG_STATE: OnceLock<Mutex<DialogSpec>> = OnceLock::new();
fn dialog_state() -> &'static Mutex<DialogSpec> {
    DIALOG_STATE.get_or_init(|| Mutex::new(DialogSpec { kind: String::new(), data: None }))
}

const MI_SETTINGS: &str = "ctx-settings";
const MI_WIDGETS: &str = "ctx-widgets";
const MI_RESTART: &str = "ctx-restart";
const MI_CLOSE: &str = "ctx-close";
#[cfg(debug_assertions)]
const MI_INSPECT: &str = "ctx-inspect";

// Workspace context menu IDs
const WS_RENAME: &str = "ws-rename";
const WS_DELETE: &str = "ws-delete";
const WS_CREATE: &str = "ws-create";
const WS_MOVE_HERE: &str = "ws-move-here";
const WS_MOVE_TO: &str = "ws-move-to-";
const WS_TOGGLE_PIN: &str = "ws-toggle-pin";
const WS_MOVE_PENDING: &str = "ws-move-pending";

pub fn build_context_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let builder = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id(MI_SETTINGS, "Settings").build(app)?)
        .item(&MenuItemBuilder::with_id(MI_WIDGETS, "Widgets").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(MI_RESTART, "Restart Bar").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(MI_CLOSE, "Close Bar").build(app)?);

    #[cfg(debug_assertions)]
    let builder = builder
        .separator()
        .item(&MenuItemBuilder::with_id(MI_INSPECT, "Inspect").build(app)?);

    builder.build()
}

fn build_workspace_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let mut builder = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id(WS_RENAME, "Rename").build(app)?);

    let ws = workspace::commands::get_workspaces();
    if ws.len() > 1 {
        builder = builder
            .item(&MenuItemBuilder::with_id(WS_DELETE, "Delete").build(app)?);
    }

    builder = builder
        .separator()
        .item(&MenuItemBuilder::with_id(WS_CREATE, "Create New Desktop").build(app)?);

    // Move Window / Pin items are **gated off** in this build. The Win32
    // `EVENT_SYSTEM_FOREGROUND` hook failed to install with
    // `ERROR_HOOK_NEEDS_HMOD` from this Rust/Tauri binary (the OS expects a
    // module-backed callback), so we have no reliable way to capture the
    // user's foreground HWND before the bar's webview child steals focus on
    // right-click. Re-enable once a polling/HWND-injection solution is in
    // place — see `workspace::foreground` for the pending implementation.
    builder = builder
        .separator()
        .item(&MenuItemBuilder::with_id(WS_MOVE_PENDING, "Move Window (Pending)").enabled(false).build(app)?);

    builder.build()
}

pub fn handle_menu_event(app: &tauri::AppHandle, id: &str) {
    eprintln!("[zenith] menu event: {}", id);
    match id {
        MI_SETTINGS => {
            let _ = create_settings_window(app);
        }
        MI_WIDGETS => {
            let _ = create_widgets_window(app);
        }
        MI_RESTART => {
            if let Some(bar) = app.get_webview_window("bar") {
                window::unregister_appbar(&bar);
                let _ = bar.hide();
            }
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe).spawn();
            }
            app.exit(0);
        }
        MI_CLOSE => {
            app.exit(0);
        }
        #[cfg(debug_assertions)]
        MI_INSPECT => {
            if let Some(bar) = app.get_webview_window("bar") {
                let _ = bar.open_devtools();
            }
        }
        WS_RENAME => {
            if DIALOG_IN_FLIGHT.swap(true, Ordering::SeqCst) {
                eprintln!("[zenith] ws-rename: guard dropped duplicate menu event");
                return;
            }
            let id = WS_CONTEXT_ID.load(Ordering::Relaxed);
            let ws = workspace::commands::get_workspaces();
            let current_name = ws.get(id as usize)
                .map(|w| w.label.clone())
                .unwrap_or_else(|| format!("Desktop {}", id + 1));
            let spec = DialogSpec {
                kind: "rename".into(),
                data: Some(serde_json::json!([id, current_name])),
            };
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                let r = show_dialog(app2, spec).await;
                DIALOG_IN_FLIGHT.store(false, Ordering::SeqCst);
                if let Err(e) = r { eprintln!("[zenith] ws-rename: dialog error {e}"); }
            });
        }
        WS_DELETE => {
            if DIALOG_IN_FLIGHT.swap(true, Ordering::SeqCst) {
                eprintln!("[zenith] ws-delete: guard dropped duplicate menu event");
                return;
            }
            let id = WS_CONTEXT_ID.load(Ordering::Relaxed);
            let spec = DialogSpec {
                kind: "delete".into(),
                data: Some(serde_json::json!(id)),
            };
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                let r = show_dialog(app2, spec).await;
                DIALOG_IN_FLIGHT.store(false, Ordering::SeqCst);
                if let Err(e) = r { eprintln!("[zenith] ws-delete: dialog error {e}"); }
            });
        }
        WS_CREATE => {
            if !workspace::commands::try_claim_action() {
                eprintln!("[zenith] ws-create: guard dropped duplicate menu event");
                return;
            }
            let _ = app.emit("zenith:workspace-create", ());
            workspace::commands::release_action();
        }
        WS_MOVE_HERE | WS_MOVE_PENDING => {
            // Move/pin are gated off in this build — see
            // `workspace::foreground` for the pending implementation and why.
            eprintln!("[zenith] ws-move-here: ignored (pending WinEvent hook fix)");
        }
        id if id.starts_with(WS_MOVE_TO) => {
            // Move/pin are gated off in this build — see
            // `workspace::foreground` for the pending implementation and why.
            eprintln!("[zenith] ws-move-to: ignored (pending WinEvent hook fix)");
        }
        WS_TOGGLE_PIN => {
            // Move/pin are gated off in this build — see
            // `workspace::foreground` for the pending implementation and why.
            eprintln!("[zenith] ws-toggle-pin: ignored (pending WinEvent hook fix)");
        }
        "vol-mute" => {
            let _ = volume::commands::set_muted(true);
        }
        "vol-unmute" => {
            let _ = volume::commands::set_muted(false);
        }
        _ => {}
    }
}

#[tauri::command]
pub fn show_workspace_context_menu(app: tauri::AppHandle, desktop_id: u32) -> Result<(), String> {
    WS_CONTEXT_ID.store(desktop_id, Ordering::Relaxed);

    let bar = app.get_webview_window("bar").ok_or("bar window not found")?;
    let menu = build_workspace_menu(&app).map_err(|e| e.to_string())?;
    bar.popup_menu(&menu).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn show_dialog(app: tauri::AppHandle, spec: DialogSpec) -> Result<(), String> {
    if let Ok(mut state) = dialog_state().lock() {
        *state = spec.clone();
    }
    tauri::async_runtime::spawn_blocking(move || create_dialog_window(&app, &spec))
        .await
        .map_err(|e| e.to_string())?
}

fn create_dialog_window(app: &tauri::AppHandle, spec: &DialogSpec) -> Result<(), String> {
    let label = format!("dialog-{}", spec.kind);
    if let Some(win) = app.get_webview_window(&label) {
        win.show().map_err(|e| e.to_string())?;
        std::thread::sleep(std::time::Duration::from_millis(500));
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    // --default-background-color=00000000 sets the WebView2 background to fully
    // transparent BEFORE the first paint, eliminating the white flash that
    // would otherwise flash for a few frames before DWM blur kicks in.
    // Format is 0xAABBGGRR (alpha-blue-green-red, little-endian).
    //
    // We also pass an `initialization_script` that defines
    // `window.__zenith_dialog_spec` BEFORE the page parse. The dialog's
    // `main.ts` reads it synchronously and renders on first paint — no IPC
    // roundtrip to `get_dialog_data`, no `await invoke(...)` latency.
    let spec_json = serde_json::to_string(spec).unwrap_or_else(|_| r#"{"kind":"unknown","data":null}"#.into());
    let init_js = format!(
        r#"window.__zenith_dialog_spec = {spec_json};
Object.freeze(window.__zenith_dialog_spec);
window.__ZENITH_DIALOG_KIND = {kind_json};
Object.freeze(window.__ZENITH_DIALOG_KIND);"#,
        spec_json = spec_json,
        kind_json = serde_json::to_string(&spec.kind).unwrap_or_else(|_| "\"unknown\"".into()),
    );
    let win = tauri::WebviewWindowBuilder::new(
        app,
        &label,
        tauri::WebviewUrl::App("dialog.html".into()),
    )
    .title(&spec.kind)
    .inner_size(320.0, 200.0)
    .min_inner_size(280.0, 120.0)
    .max_inner_size(600.0, 600.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .visible(false) // shown by SWP_SHOWWINDOW *after* accent policy + corners, no flash
    .focused(true)
    .additional_browser_args("--default-background-color=00000000")
    .initialization_script(&init_js)
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window::apply_fixed_acrylic(app, &label);
    let _ = window::set_rounded_corners(&win);
    let _ = window::set_disable_transitions(&win);

    // Show AFTER the materials are applied so DWM blur is ready before pixels hit
    // the screen. Dropping NOACTIVATE ensures the new window is actually
    // foregrounded — set_focus() alone races the Windows foreground rules and
    // often loses (popup appears but is not focused).
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_SHOWWINDOW, SWP_NOZORDER, SWP_NOSIZE, SWP_NOMOVE};
    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    let _ = unsafe { SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOSIZE | SWP_NOMOVE) };
    std::thread::sleep(std::time::Duration::from_millis(500));
    let _ = win.set_focus();
    Ok(())
}

#[tauri::command]
pub fn get_dialog_data() -> Result<(String, Option<serde_json::Value>), String> {
    dialog_state()
        .lock()
        .map(|s| (s.kind.clone(), s.data.clone()))
        .map_err(|e| e.to_string())
}

fn create_settings_window(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        eprintln!("[zenith] create_settings: showing existing");
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
    } else {
        eprintln!("[zenith] create_settings: building new window");
        let win = tauri::WebviewWindowBuilder::new(
            app,
            "settings",
            tauri::WebviewUrl::App("settings.html".into()),
        )
        .title("Zenith — Settings")
        .inner_size(435.0, 600.0)
        .min_inner_size(435.0, 300.0)
        .max_inner_size(800.0, 600.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .center()
        .visible(false)
        .focused(true)
        .additional_browser_args("--default-background-color=00000000")
        .build()
        .map_err(|e| e.to_string())?;
        eprintln!("[zenith] create_settings: built, applying material");

        let _ = window::apply_fixed_acrylic(app, "settings");
        let _ = window::set_rounded_corners(&win);
        let _ = window::set_disable_transitions(&win);
        // Show *after* the material is applied, so DWM blur is ready before
        // any pixels hit the screen — eliminates the white-flash race. Drop
        // NOACTIVATE so the window actually takes foreground on open.
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_SHOWWINDOW, SWP_NOZORDER, SWP_NOSIZE, SWP_NOMOVE};
        let hwnd = win.hwnd().map_err(|e| e.to_string())?;
        let _ = unsafe { SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOSIZE | SWP_NOMOVE) };
        let _ = win.set_focus();
        eprintln!("[zenith] create_settings: done");
    }
    Ok(())
}

fn create_widgets_window(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("widgets") {
        eprintln!("[zenith] create_widgets: showing existing");
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
    } else {
        eprintln!("[zenith] create_widgets: building new window");
        let win = tauri::WebviewWindowBuilder::new(
            app,
            "widgets",
            tauri::WebviewUrl::App("widgets.html".into()),
        )
        .title("Zenith — Widgets")
        .inner_size(800.0, 600.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .center()
        .visible(false)
        .focused(true)
        .additional_browser_args("--default-background-color=00000000")
        .build()
        .map_err(|e| e.to_string())?;
        eprintln!("[zenith] create_widgets: built, applying material");

        let _ = window::apply_fixed_acrylic(app, "widgets");
        eprintln!("[zenith] create_widgets: material applied");
        let _ = window::set_rounded_corners(&win);
        let _ = window::set_disable_transitions(&win);
        // Show *after* the material is applied, so DWM blur is ready before
        // any pixels hit the screen — eliminates the white-flash race. Drop
        // NOACTIVATE so the window actually takes foreground on open.
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_SHOWWINDOW, SWP_NOZORDER, SWP_NOSIZE, SWP_NOMOVE};
        let hwnd = win.hwnd().map_err(|e| e.to_string())?;
        let _ = unsafe { SetWindowPos(hwnd, None, 0, 0, 0, 0, SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOSIZE | SWP_NOMOVE) };
        let _ = win.set_focus();
        eprintln!("[zenith] create_widgets: done");
    }
    Ok(())
}

#[tauri::command]
pub async fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || create_settings_window(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn open_widgets(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || create_widgets_window(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn show_context_menu(app: tauri::AppHandle) -> Result<(), String> {
    let bar = app.get_webview_window("bar").ok_or("bar window not found")?;
    let menu = build_context_menu(&app).map_err(|e| e.to_string())?;
    bar.popup_menu(&menu).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn open_widget_config(app: tauri::AppHandle, widget_id: String) -> Result<(), String> {
    let label = format!("widget-config-{}", widget_id);
    if app.get_webview_window(&label).is_some() {
        return Ok(());
    }
    let init_js = format!(
        r#"window.__ZENITH_WIDGET_CONFIG_ID = {};"#,
        serde_json::to_string(&widget_id).unwrap_or_else(|_| "\"\"".into())
    );
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let win = tauri::WebviewWindowBuilder::new(
            &app,
            &label,
            tauri::WebviewUrl::App("widget-config.html".into()),
        )
        .title("Widget Settings")
        .inner_size(400.0, 600.0)
        .min_inner_size(400.0, 200.0)
        .max_inner_size(600.0, 800.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .center()
        .visible(false)
        .focused(true)
        .additional_browser_args("--default-background-color=00000000")
        .initialization_script(&init_js)
        .build()
        .map_err(|e| e.to_string())?;

        let _ = window::apply_fixed_acrylic(&app, &label);
        let _ = window::set_rounded_corners(&win);
        let _ = window::set_disable_transitions(&win);

        // Drop NOACTIVATE so the new window actually takes foreground —
        // otherwise the trailing set_focus() races Windows' foreground rules
        // and the window stays unfocused on open.
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
        };
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
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = win.set_focus();
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
