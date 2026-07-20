//! Link icons stored on disk (one PNG per link).
//!
//! Icons live at `<APPDATA>\zenith\icons\<link_id>.png`. Stored as PNG so:
//!   - Tauri's `Image::from_bytes` decodes them (used for window icons).
//!   - WebView2's `<img>` renders them (used by the bar widget).
//!   - Alpha transparency is preserved.
//!
//! Incoming formats (PNG/JPEG/WebP/GIF/BMP/ICO) are converted to PNG on save
//! via the `image` crate. This is necessary because Tauri's image-png-only
//! feature set can't decode WebP/JPEG/etc. directly — so a user uploading a
//! WebP logo would otherwise see the default window icon.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use base64::Engine;

use crate::config::repository::config_dir;

const ICONS_SUBDIR: &str = "icons";

pub fn icons_dir() -> PathBuf {
    let dir = config_dir().join(ICONS_SUBDIR);
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn link_icon_path(id: &str) -> PathBuf {
    icons_dir().join(format!("{id}.png"))
}

/// Save a `data:` URL icon to disk as PNG. Decodes the source format
/// (PNG/JPEG/WebP/GIF/BMP/ICO) via the `image` crate, then re-encodes as
/// PNG with full alpha. Atomic write.
pub fn save_link_icon(id: &str, data_url: &str) -> Result<(), String> {
    let rest = data_url
        .strip_prefix("data:")
        .ok_or_else(|| "not a data: URL".to_string())?;
    let (meta, encoded) = rest
        .split_once(',')
        .ok_or_else(|| "malformed data: URL (no comma)".to_string())?;
    if !meta.to_ascii_lowercase().contains(";base64") {
        return Err("only base64 data: URLs are supported".into());
    }
    let mime = meta.split(';').next().unwrap_or("");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("base64 decode: {e}"))?;

    let format = match mime {
        "image/png" => image::ImageFormat::Png,
        "image/jpeg" | "image/jpg" => image::ImageFormat::Jpeg,
        "image/webp" => image::ImageFormat::WebP,
        "image/gif" => image::ImageFormat::Gif,
        "image/bmp" => image::ImageFormat::Bmp,
        "image/x-icon" | "image/vnd.microsoft.icon" => image::ImageFormat::Ico,
        other => return Err(format!("unsupported icon mime type: {other}")),
    };

    let img = image::load_from_memory_with_format(&bytes, format)
        .map_err(|e| format!("image decode: {e}"))?;

    let mut png_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| format!("png encode: {e}"))?;

    let path = link_icon_path(id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("png.tmp");
    fs::write(&tmp, &png_bytes).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("rename → {}: {e}", path.display()))?;
    Ok(())
}

pub fn delete_link_icon(id: &str) -> Result<(), String> {
    let path = link_icon_path(id);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("remove {}: {e}", path.display()))?;
    }
    Ok(())
}

/// Read saved PNG bytes. None if no icon is configured for this link.
pub fn read_link_icon_png(id: &str) -> Option<Vec<u8>> {
    fs::read(link_icon_path(id)).ok()
}

/// Read as `data:image/png;base64,...` for use as `<img src>` in the bar widget.
pub fn read_link_icon_data_url(id: &str) -> Option<String> {
    let bytes = read_link_icon_png(id)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:image/png;base64,{encoded}"))
}

/// Load as a Tauri `Image` for use as a window icon. PNG only — works with
/// Tauri's `image-png` feature without requiring WebP/JPEG/etc. support.
/// Uses `from_path` so the returned `Image<'static>` owns its decoded bytes.
pub fn load_link_icon_image(id: &str) -> Option<tauri::image::Image<'static>> {
    let path = link_icon_path(id);
    if !path.exists() {
        return None;
    }
    tauri::image::Image::from_path(&path).ok()
}

#[allow(dead_code)]
pub fn has_link_icon(id: &str) -> bool {
    link_icon_path(id).exists()
}

/// One-time startup migration: any `data:` URL stored in the legacy
/// `widgets.config.links.links[].icon` field is decoded + saved to disk as
/// PNG, then the field is cleared. Idempotent — no-ops once migration is done.
pub fn migrate_legacy_data_urls() {
    let mut cfg = crate::config::load();
    let mut changed = false;

    let Some(links_widget) = cfg.widgets.config.get_mut("links") else {
        return;
    };
    let Some(arr) = links_widget
        .get_mut("links")
        .and_then(|v| v.as_array_mut())
    else {
        return;
    };

    for item in arr.iter_mut() {
        let Some(obj) = item.as_object_mut() else { continue; };
        let Some(icon_str) = obj.get("icon").and_then(|v| v.as_str()) else { continue; };
        if !icon_str.starts_with("data:") {
            continue;
        }
        let Some(id) = obj
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        else {
            continue;
        };

        match save_link_icon(&id, icon_str) {
            Ok(()) => {
                obj.insert("icon".into(), serde_json::Value::Null);
                changed = true;
                eprintln!("[webapp] migrated icon for link {id} → disk");
            }
            Err(e) => {
                eprintln!("[webapp] icon migration failed for link {id}: {e}");
            }
        }
    }

    if changed {
        let _ = crate::config::repository::save(&cfg);
    }
}
