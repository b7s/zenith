use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::shared::{AppError, AppResult};

use super::model::Config;

/// Resolve `%APPDATA%\zenith\config.json`.
/// Falls back to `<temp>\zenith\config.json` if APPDATA is unset.
pub fn config_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("zenith")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

/// The safe getter. Always returns a usable `Config`.
///
/// - file missing        -> Config::default()
/// - file unreadable     -> Config::default()  (logs)
/// - file invalid JSON   -> Config::default()  (logs)
/// - file valid          -> parsed, missing keys filled by serde defaults
///
/// Never panics, never returns Result. Call this everywhere config is needed.
pub fn load() -> Config {
    load_at(&config_path())
}

pub fn load_at(path: &Path) -> Config {
    match try_load_at(path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("[zenith] config load failed ({e}); using defaults");
            Config::default()
        }
    }
}

fn try_load_at(path: &Path) -> AppResult<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let cfg: Config =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(cfg)
}

/// Read one value by JSON pointer path with a caller-supplied fallback.
/// Example: `get_or("/appearance/bar_height", 40)`.
#[allow(dead_code)]
pub fn get_or<T>(pointer: &str, fallback: T) -> T
where
    T: for<'de> serde::Deserialize<'de>,
{
    let cfg = load();
    let raw = serde_json::to_value(&cfg).unwrap_or(serde_json::Value::Null);
    match raw.pointer(pointer) {
        Some(v) => serde_json::from_value(v.clone()).unwrap_or(fallback),
        None => fallback,
    }
}

/// Merge-on-save: keep unknown keys from the existing file so manual edits
/// (and future fields) are not lost when an older build writes config back.
pub fn save(cfg: &Config) -> AppResult<()> {
    save_at(&config_path(), cfg)
}

pub fn save_at(path: &Path, cfg: &Config) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut value = serde_json::to_value(cfg)?;
    if let Ok(existing) = fs::read_to_string(path) {
        if let Ok(prev) = serde_json::from_str::<serde_json::Value>(&existing) {
            if let (Some(prev_obj), Some(new_obj)) = (prev.as_object(), value.as_object_mut()) {
                for (k, v) in prev_obj {
                    new_obj.entry(k).or_insert(v.clone());
                }
            }
        }
    }

    let json = serde_json::to_string_pretty(&value)?;
    atomic_write(path, json.as_bytes())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        use io::Write;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path).map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::model::{AppearanceConfig, BackgroundConfig};

    fn unique_path(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zenith-test-{}-{tag}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("config.json")
    }

    #[test]
    fn missing_file_yields_defaults() {
        let path = unique_path("missing");
        let cfg = load_at(&path);
        assert_eq!(cfg.appearance.background.mode, "acrylic");
        assert_eq!(cfg.appearance.bar_height, 40);
    }

    #[test]
    fn malformed_json_yields_defaults() {
        let path = unique_path("malformed");
        fs::write(&path, b"{ not json").unwrap();
        let cfg = load_at(&path);
        assert_eq!(cfg.appearance.background.mode, "acrylic");
    }

    #[test]
    fn save_then_load_roundtrip() {
        let path = unique_path("roundtrip");
        let original = Config {
            appearance: AppearanceConfig {
                background: BackgroundConfig { mode: "mica".into(), ..Default::default() },
                bar_height: 52,
                ..Default::default()
            },
            ..Default::default()
        };
        save_at(&path, &original).unwrap();
        let loaded = load_at(&path);
        assert_eq!(loaded.appearance.background.mode, "mica");
        assert_eq!(loaded.appearance.bar_height, 52);
        assert_eq!(loaded.appearance.theme, "auto");
    }

    #[test]
    fn get_or_returns_fallback_for_missing_pointer() {
        assert_eq!(get_or("/does/not/exist", 99u32), 99);
    }
}
