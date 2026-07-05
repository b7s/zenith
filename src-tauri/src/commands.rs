use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::Manager;

use crate::window;

const MI_SETTINGS: &str = "ctx-settings";
const MI_WIDGETS: &str = "ctx-widgets";
const MI_RESTART: &str = "ctx-restart";
const MI_CLOSE: &str = "ctx-close";
const MI_INSPECT: &str = "ctx-inspect";

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

pub fn handle_menu_event(app: &tauri::AppHandle, id: &str) {
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
        _ => {}
    }
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
        .visible(true)
        .focused(true)
        .build()
        .map_err(|e| e.to_string())?;
        eprintln!("[zenith] create_settings: built, applying material");

        let _ = window::apply_fixed_acrylic(app, "settings");
        let _ = window::set_rounded_corners(&win);
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
        .visible(true)
        .focused(true)
        .build()
        .map_err(|e| e.to_string())?;
        eprintln!("[zenith] create_widgets: built, applying material");

        let _ = window::apply_fixed_acrylic(app, "widgets");
        eprintln!("[zenith] create_widgets: material applied");
        let _ = window::set_rounded_corners(&win);
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
