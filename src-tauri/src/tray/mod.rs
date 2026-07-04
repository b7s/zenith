use tauri::AppHandle;
use tauri::Manager;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{TrayIconBuilder, TrayIconEvent, MouseButton};

pub fn create(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("ctx-settings", "Settings").build(app)?)
        .item(&MenuItemBuilder::with_id("ctx-widgets", "Widgets").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("ctx-restart", "Restart Bar").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("ctx-close", "Close Bar").build(app)?)
        .build()?;

    TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| {
            crate::commands::handle_menu_event(app, event.id().as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(event, TrayIconEvent::Click { button: MouseButton::Left, .. }) {
                if let Some(window) = tray.app_handle().get_webview_window("bar") {
                    if window.is_visible().ok().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
