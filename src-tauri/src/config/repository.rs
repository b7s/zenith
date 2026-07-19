use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::shared::{AppError, AppResult, sync};

use super::model::Config;

/// Bare config file name. Shared between local + OneDrive path resolution
/// so the same file lives at `<APPDATA>\zenith\config.json` and (when sync
/// is enabled) at `<OneDrive>\Zenith\config.json`.
const FILE_NAME: &str = "config.json";

/// Resolve `%APPDATA%\zenith\`.
/// Falls back to `<temp>\zenith\` if APPDATA is unset.
pub fn config_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("zenith")
}

pub fn config_path() -> PathBuf {
    config_dir().join(FILE_NAME)
}

/// The safe getter. Always returns a usable `Config`.
///
/// Resolution order (per the storage contract):
///   1. Local file exists        → parse it (missing keys filled by serde defaults)
///   2. OneDrive file exists     → copy it to local, return it
///   3. Neither exists           → seed defaults to local (no OneDrive write —
///      defaults have `storage.onedrive_sync_enabled=false`)
///
/// Never panics, never returns Result. Call this everywhere config is needed.
pub fn load() -> Config {
    let local = config_path();
    if local.exists() {
        return match try_load_at(&local) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("[zenith:config] local load failed ({e}); using defaults");
                Config::default()
            }
        };
    }
    // Local missing — try OneDrive (works regardless of the toggle because
    // we cannot read the toggle without a local config; this is the roaming
    // bootstrap case where the user installed Zenith on a new machine).
    if let Some(remote) = sync::onedrive_path_for(FILE_NAME) {
        match sync::read_json::<Config>(&remote) {
            Ok(Some(cfg)) => {
                // Seed the local file from the remote copy so subsequent
                // loads hit the fast local-only path.
                let _ = save_at(&local, &cfg);
                return cfg;
            }
            Ok(None) => { /* fall through to defaults */ }
            Err(e) => eprintln!("[zenith:config] onedrive read failed: {e}"),
        }
    }
    // Neither exists — seed defaults locally. We deliberately do NOT push
    // to OneDrive here: defaults carry `onedrive_sync_enabled=false`, so a
    // fresh machine never accidentally creates a OneDrive file. The user
    // opts in via Settings → General, and the very save that flips the
    // toggle is the one that creates the remote file.
    let cfg = Config::default();
    let _ = save_at(&local, &cfg);
    cfg
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

/// Persist config: merge-on-save to preserve unknown keys, write atomically
/// to the local file, then push to OneDrive when sync is enabled. The
/// OneDrive write is best-effort — a missing/unmounted OneDrive never
/// breaks the local save.
pub fn save(cfg: &Config) -> AppResult<()> {
    save_at(&config_path(), cfg)?;
    let _ = sync::push_to_onedrive(FILE_NAME, cfg, cfg.storage.onedrive_sync_enabled);
    Ok(())
}

/// Merge-on-save: keep unknown keys from the existing file so manual edits
/// (and future fields) are not lost when an older build writes config back.
/// Local-only — never touches OneDrive (used by `save()` for the local step
/// and by `load()` for seeding).
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
        let cfg = Config::default();
        // Mirror `load()`'s seed step without touching OneDrive.
        save_at(&path, &cfg).unwrap();
        let loaded = try_load_at(&path).unwrap();
        assert_eq!(loaded.appearance.background.mode, "gradient");
        assert_eq!(loaded.appearance.background.color_top, "#1f2541");
        assert_eq!(loaded.appearance.background.color_bottom, "#1a1a1a");
        assert_eq!(loaded.appearance.background.alpha_top, 60);
        assert_eq!(loaded.appearance.background.alpha_bottom, 0);
        assert_eq!(loaded.appearance.tint_alpha, 61);
        assert_eq!(loaded.appearance.bar_height, 40);
    }

    #[test]
    fn save_at_creates_file() {
        let path = unique_path("seed");
        assert!(!path.exists());
        let cfg = Config::default();
        save_at(&path, &cfg).unwrap();
        assert!(path.exists());
        let loaded = try_load_at(&path).unwrap();
        assert_eq!(loaded.appearance.background.mode, "gradient");
    }

    #[test]
    fn malformed_json_yields_defaults() {
        let path = unique_path("malformed");
        fs::write(&path, b"{ not json").unwrap();
        let cfg = try_load_at(&path).unwrap_or_default();
        assert_eq!(cfg.appearance.background.mode, "gradient");
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
        let loaded = try_load_at(&path).unwrap();
        assert_eq!(loaded.appearance.background.mode, "mica");
        assert_eq!(loaded.appearance.bar_height, 52);
        assert_eq!(loaded.appearance.theme, "dark");
    }

    #[test]
    fn get_or_returns_fallback_for_missing_pointer() {
        assert_eq!(get_or("/does/not/exist", 99u32), 99);
    }
}
