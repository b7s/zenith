use tauri::{AppHandle, Emitter};

use crate::shared::EVENT_CONFIG_UPDATED;

use super::{model::Config, repository};

#[tauri::command]
pub fn get_config() -> Config {
    repository::load()
}

#[tauri::command]
pub fn save_config(app: AppHandle, config: Config) -> Result<bool, String> {
    repository::save(&config).map_err(|e| e.to_string())?;
    app.emit(EVENT_CONFIG_UPDATED, &config).ok();
    Ok(true)
}
