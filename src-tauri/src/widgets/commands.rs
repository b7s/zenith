use serde::Serialize;
use tauri::AppHandle;

use super::{manifest::WidgetManifest, registry};

#[derive(Debug, Clone, Serialize)]
pub struct WidgetSource {
    pub html: String,
    pub css: String,
    pub js: String,
}

#[tauri::command]
pub fn get_widgets(app: AppHandle) -> Vec<WidgetManifest> {
    registry::scan_widgets(&app)
}

#[tauri::command]
pub fn get_widget_source(app: AppHandle, id: String) -> Option<WidgetSource> {
    registry::widget_source(&app, &id).map(|s| WidgetSource {
        html: s.html,
        css: s.css,
        js: s.js,
    })
}
