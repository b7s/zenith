use std::sync::atomic::{AtomicU32, Ordering};
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::Emitter;
use tauri::Manager;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_YESNO, MB_ICONQUESTION, IDYES};
use windows::core::HSTRING;

use crate::window;
use crate::workspace;

static WS_CONTEXT_ID: AtomicU32 = AtomicU32::new(0);

const MI_SETTINGS: &str = "ctx-settings";
const MI_WIDGETS: &str = "ctx-widgets";
const MI_RESTART: &str = "ctx-restart";
const MI_CLOSE: &str = "ctx-close";
const MI_INSPECT: &str = "ctx-inspect";

// Workspace context menu IDs
const WS_RENAME: &str = "ws-rename";
const WS_DELETE: &str = "ws-delete";
const WS_CREATE: &str = "ws-create";
const WS_MOVE_HERE: &str = "ws-move-here";
const WS_MOVE_TO: &str = "ws-move-to-";
const WS_TOGGLE_PIN: &str = "ws-toggle-pin";

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

    // Check for foreground window (use cached HWND captured before menu opened)
    let hwnd = workspace::commands::get_cached_foreground_hwnd();
    let has_window = !hwnd.is_invalid();

    if has_window {
        builder = builder
            .separator()
            .item(&MenuItemBuilder::with_id(WS_MOVE_HERE, "Move Window Here").build(app)?);

        // Build "Move Window To" submenu
        let mut sub = SubmenuBuilder::new(app, "Move Window To");
        for w in &ws {
            let label = if w.label.is_empty() {
                format!("Desktop {}", w.id + 1)
            } else {
                w.label.clone()
            };
            sub = sub.item(&MenuItemBuilder::with_id(format!("{}{}", WS_MOVE_TO, w.id), label).build(app)?);
        }
        builder = builder.item(&sub.build()?);

        // Check current pin state
        let pin_state = workspace::commands::pin_state();
        let pin_label = if pin_state { "Unpin Window From All Desktops" } else { "Pin Window To All Desktops" };
        builder = builder
            .separator()
            .item(&MenuItemBuilder::with_id(WS_TOGGLE_PIN, pin_label).build(app)?);
    }

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
        WS_RENAME => {
            let id = WS_CONTEXT_ID.load(Ordering::Relaxed);
            let _ = app.emit("zenith:workspace-rename", id);
        }
        WS_DELETE => {
            let id = WS_CONTEXT_ID.load(Ordering::Relaxed);
            let _ = app.emit("zenith:workspace-delete", id);
        }
        WS_CREATE => {
            let _ = app.emit("zenith:workspace-create", ());
        }
        WS_MOVE_HERE => {
            let id = WS_CONTEXT_ID.load(Ordering::Relaxed);
            let _ = app.emit("zenith:workspace-move-here", id);
        }
        id if id.starts_with(WS_MOVE_TO) => {
            if let Ok(index) = id[WS_MOVE_TO.len()..].parse::<u32>() {
                let _ = app.emit("zenith:workspace-move-to", index);
            }
        }
        WS_TOGGLE_PIN => {
            let _ = app.emit("zenith:workspace-toggle-pin", ());
        }
        _ => {}
    }
}

#[tauri::command]
pub fn show_workspace_context_menu(app: tauri::AppHandle) -> Result<(), String> {
    // Capture the real foreground window before the bar takes focus
    let fg = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    workspace::commands::set_foreground_hwnd(fg.0);

    let bar = app.get_webview_window("bar").ok_or("bar window not found")?;
    let menu = build_workspace_menu(&app).map_err(|e| e.to_string())?;
    bar.popup_menu(&menu).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn confirm_delete_desktop(app: tauri::AppHandle, id: u32) -> Result<bool, String> {
    let title = HSTRING::from("Delete Desktop");
    let msg = HSTRING::from(format!("Delete Desktop {}?\nWindows will be moved to another desktop.", id + 1));

    let result = tauri::async_runtime::spawn_blocking(move || {
        unsafe {
            MessageBoxW(None, &msg, &title, MB_YESNO | MB_ICONQUESTION)
        }
    }).await.map_err(|e| e.to_string())?;

    if result == IDYES {
        workspace::commands::delete_desktop(app, id).map(|_| true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
pub fn show_rename_dialog(app: tauri::AppHandle, id: u32, current_name: String) -> Result<(), String> {
    let label = format!("rename-{}", id);
    if let Some(win) = app.get_webview_window(&label) {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    let encoded = urlencoding(&current_name);
    let url = format!("rename.html?id={}&name={}", id, encoded);
    let win = tauri::WebviewWindowBuilder::new(
        &app,
        &label,
        tauri::WebviewUrl::App(url.into()),
    )
    .title("Rename Desktop")
    .inner_size(320.0, 140.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .center()
    .visible(true)
    .focused(true)
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window::apply_fixed_acrylic(&app, &label);
    let _ = window::set_rounded_corners(&win);
    Ok(())
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
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
