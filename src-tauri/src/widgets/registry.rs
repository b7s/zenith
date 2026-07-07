use std::path::PathBuf;

use super::manifest::WidgetManifest;

fn widgets_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    for candidate in &[cwd.join("widgets"), cwd.join("../widgets")] {
        if candidate.is_dir() {
            return candidate.clone();
        }
    }
    cwd.join("widgets")
}

pub fn scan_widgets() -> Vec<WidgetManifest> {
    let dir = widgets_dir();
    if !dir.is_dir() {
        eprintln!("[zenith] widgets dir not found: {}", dir.display());
        return vec![];
    }

    let mut widgets = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return widgets,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }
        match std::fs::read_to_string(&manifest_path) {
            Ok(raw) => match serde_json::from_str::<WidgetManifest>(&raw) {
                Ok(mut m) => {
                    m.widget_dir = path.file_name().unwrap_or_default().to_string_lossy().into();
                    if m.id.is_empty() {
                        m.id = m.widget_dir.clone();
                    }
                    if m.name.is_empty() {
                        m.name = m.id.clone();
                    }
                    widgets.push(m);
                }
                Err(e) => eprintln!("[zenith] bad manifest {}: {e}", manifest_path.display()),
            },
            Err(e) => eprintln!("[zenith] cannot read {}: {e}", manifest_path.display()),
        }
    }
    widgets
}

#[derive(Debug, Clone)]
pub struct WidgetSource {
    pub html: String,
    pub css: String,
    pub js: String,
}

pub fn widget_source(id: &str) -> Option<WidgetSource> {
    let dir = widget_dir_for_id(id)?;
    let html = std::fs::read_to_string(dir.join("widget.html")).ok().unwrap_or_default();
    let css = std::fs::read_to_string(dir.join("widget.css")).ok().unwrap_or_default();
    let js = std::fs::read_to_string(dir.join("widget.js")).ok().unwrap_or_default();
    Some(WidgetSource { html, css, js })
}

/// Resolve the widget directory for a given `id`. First tries `widgets/<id>/`,
/// then falls back to scanning manifests to map `id` → `widget_dir`.
fn widget_dir_for_id(id: &str) -> Option<PathBuf> {
    let base = widgets_dir();
    let direct = base.join(id);
    if direct.is_dir() {
        return Some(direct);
    }
    // Fallback: scan manifests to find a widget with matching id and use its widget_dir
    for m in scan_widgets() {
        if m.id == id {
            let dir = base.join(&m.widget_dir);
            if dir.is_dir() {
                return Some(dir);
            }
        }
    }
    None
}
