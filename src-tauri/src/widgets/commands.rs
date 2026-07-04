use serde::Serialize;

use super::{manifest::WidgetManifest, registry};

#[derive(Debug, Clone, Serialize)]
pub struct WidgetSource {
    pub html: String,
    pub css: String,
    pub js: String,
}

#[tauri::command]
pub fn get_widgets() -> Vec<WidgetManifest> {
    registry::scan_widgets()
}

#[tauri::command]
pub fn get_widget_source(id: String) -> Option<WidgetSource> {
    registry::widget_source(&id).map(|s| WidgetSource {
        html: s.html,
        css: s.css,
        js: s.js,
    })
}
