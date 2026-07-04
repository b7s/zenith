use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::Manager;

use crate::window;

const MI_SETTINGS: &str = "ctx-settings";
const MI_WIDGETS: &str = "ctx-widgets";
const MI_RESTART: &str = "ctx-restart";
const MI_CLOSE: &str = "ctx-close";

pub fn build_context_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id(MI_SETTINGS, "Settings").build(app)?)
        .item(&MenuItemBuilder::with_id(MI_WIDGETS, "Widgets").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(MI_RESTART, "Restart Bar").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(MI_CLOSE, "Close Bar").build(app)?)
        .build()
}

pub fn handle_menu_event(app: &tauri::AppHandle, id: &str) {
    match id {
        MI_SETTINGS => {
            let _ = open_settings(app.clone());
        }
        MI_WIDGETS => {
            let _ = open_widgets(app.clone());
        }
        MI_RESTART => {
            if let Some(bar) = app.get_webview_window("bar") {
                window::unregister_appbar(&bar);
            }
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe).spawn();
            }
            app.exit(0);
        }
        MI_CLOSE => {
            app.exit(0);
        }
        _ => {}
    }
}

#[tauri::command]
pub fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
    } else {
        let win = tauri::WebviewWindowBuilder::new(
            &app,
            "settings",
            tauri::WebviewUrl::App("settings.html".into()),
        )
        .title("Zenith — Settings")
        .inner_size(800.0, 600.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .center()
        .visible(true)
        .focused(true)
        .build()
        .map_err(|e| e.to_string())?;

        let _ = window::apply_material(&app, "settings");
        let _ = window::set_rounded_corners(&win);
    }
    Ok(())
}

#[tauri::command]
pub fn open_widgets(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("widgets") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
    } else {
        let win = tauri::WebviewWindowBuilder::new(
            &app,
            "widgets",
            tauri::WebviewUrl::App("widgets.html".into()),
        )
        .title("Zenith — Widgets")
        .inner_size(800.0, 600.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .center()
        .visible(true)
        .focused(true)
        .build()
        .map_err(|e| e.to_string())?;

        let _ = window::apply_material(&app, "widgets");
        let _ = window::set_rounded_corners(&win);
    }
    Ok(())
}

/// Show the bar's right-click context menu at the cursor.
///
/// This uses Tauri's native `popup_menu` rather than a Win32 `TrackPopupMenu`
/// modal loop. The previous implementation ran a blocking Win32 modal pump on
/// the Tauri main thread, which stalled the IPC channel and froze other
/// windows (notably Settings/Widgets, whose `mountWindow` awaited `get_config`
/// and never completed — leaving them blank and unclosable).
#[tauri::command]
pub fn show_context_menu(app: tauri::AppHandle) -> Result<(), String> {
    let bar = app.get_webview_window("bar").ok_or("bar window not found")?;
    let menu = build_context_menu(&app).map_err(|e| e.to_string())?;
    bar.popup_menu(&menu).map_err(|e| e.to_string())?;
    Ok(())
}
