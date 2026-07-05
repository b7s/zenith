use tauri::{AppHandle, Emitter, Manager};

use crate::shared::EVENT_CONFIG_UPDATED;
use crate::window;

use super::{model::Config, repository};

#[tauri::command]
pub fn get_config() -> Config {
    repository::load()
}

#[tauri::command]
pub fn save_config(app: AppHandle, config: Config) -> Result<bool, String> {
    repository::save(&config).map_err(|e| e.to_string())?;
    let _ = window::apply_material(&app, "bar");
    if let Some(bar) = app.get_webview_window("bar") {
        let _ = window::update_appbar(&bar);
    }
    app.emit(EVENT_CONFIG_UPDATED, &config).ok();
    Ok(true)
}
