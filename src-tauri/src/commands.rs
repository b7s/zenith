use tauri::Manager;

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
        .build()
        .map_err(|e| e.to_string())?;

        let _ = crate::window::apply_material(&app, "settings");
        let _ = crate::window::set_rounded_corners(&win);
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
        .build()
        .map_err(|e| e.to_string())?;

        let _ = crate::window::apply_material(&app, "widgets");
        let _ = crate::window::set_rounded_corners(&win);
    }
    Ok(())
}
