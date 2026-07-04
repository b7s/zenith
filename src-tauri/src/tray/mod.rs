use tauri::Manager;
use tauri::AppHandle;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{TrayIconBuilder, TrayIconEvent, MouseButton};

pub fn create(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id("settings", "Settings").build(app)?)
        .item(&MenuItemBuilder::with_id("widgets", "Widgets").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("restart", "Restart Bar").build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id("close", "Close Bar").build(app)?)
        .build()?;

    TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "settings" => {
                    let _ = crate::commands::open_settings(app.clone());
                }
                "widgets" => {
                    let _ = crate::commands::open_widgets(app.clone());
                }
                "restart" => {
                    eprintln!("[zenith] restart requested");
                    if let Ok(exe) = std::env::current_exe() {
                        let _ = std::process::Command::new(exe)
                            .spawn()
                            .map(|_| app.exit(0));
                    }
                }
                "close" => {
                    app.exit(0);
                }
                _ => {}
            }
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
