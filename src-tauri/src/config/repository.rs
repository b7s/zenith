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
    let mut value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    let migrated = migrate(&mut value);
    let cfg: Config = serde_json::from_value(value)
        .map_err(|e| format!("re-parse after migration {}: {e}", path.display()))?;
    if migrated {
        // Persist the migration so subsequent loads don't re-run the same
        // dance. Best-effort — a write failure here doesn't break the load.
        if let Ok(s) = serde_json::to_string_pretty(&serde_json::to_value(&cfg)?) {
            let _ = atomic_write(path, s.as_bytes());
        }
    }
    Ok(cfg)
}

/// One-shot, in-place config migrations. Each block targets a specific
/// stale identifier / shape from a previously-removed or renamed feature
/// so users who had it enabled don't lose their setup.
///
/// **Migrations must be safe to re-run and must be additive / renaming only
/// — never destructive of legitimate user state.** Add a NEW block when
/// introducing a rename; never modify an existing one's behaviour (that
/// would re-run the migration on already-migrated files forever).
fn migrate(value: &mut serde_json::Value) -> bool {
    let mut changed = false;

    // 2026-07-23 — AI Agents widget id renamed from `ai-cli` (hyphenated
    // ghost build residue) to `ai_cli`. Rename in `enabled` and
    // `positions`, and drop any stray `widgets.config["ai-cli"]` block so
    // the bar re-enables the canonical widget with its first-run config.
    {
        let widgets = value
            .as_object_mut()
            .and_then(|o| o.get_mut("widgets"))
            .and_then(|w| w.as_object_mut());
        if let Some(widgets_obj) = widgets {
            if let Some(arr) = widgets_obj.get_mut("enabled").and_then(|e| e.as_array_mut()) {
                let original_len = arr.len();
                let mut new_arr: Vec<serde_json::Value> = Vec::with_capacity(arr.len());
                for v in arr.iter() {
                    if v.as_str() == Some("ai-cli") {
                        new_arr.push(serde_json::Value::String("ai_cli".into()));
                    } else {
                        new_arr.push(v.clone());
                    }
                }
                *arr = new_arr;
                if arr.len() != original_len || widgets_obj.contains_key("ai-cli") {
                    changed = true;
                }
            }
            if let Some(positions) = widgets_obj
                .get_mut("positions")
                .and_then(|p| p.as_object_mut())
            {
                let remove_key = positions.remove("ai-cli").is_some();
                if remove_key {
                    if !positions.contains_key("ai_cli") {
                        // Default placement for the canonical widget — same
                        // side the ghost was on so the user's bar layout
                        // doesn't visibly jump.
                        positions.insert(
                            "ai_cli".into(),
                            serde_json::Value::String("left".into()),
                        );
                    }
                    changed = true;
                }
            }
            // Drop only the leftover config sub-block for the removed ghost
            // id. **Never** remove the whole `config` object — that would
            // wipe every widget's saved settings (weather location,
            // system_stats toggles, media prefs, etc.).
            if let Some(cfg_obj) = widgets_obj
                .get_mut("config")
                .and_then(|c| c.as_object_mut())
            {
                if cfg_obj.remove("ai-cli").is_some() {
                    changed = true;
                }
            }
        }
    }

    changed
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

    /// Migrates the legacy `ai-cli` ghost id to the canonical `ai_cli`:
    /// in `enabled`, `positions`, and the leftover `config["ai-cli"]` block.
    #[test]
    fn migration_renames_ai_cli_to_ai_cli_with_separator() {
        let mut raw = serde_json::json!({
            "widgets": {
                "enabled": [
                    "workspace",
                    "media",
                    "ai-cli",
                    "git"
                ],
                "positions": {
                    "ai-cli": "left",
                    "workspace": "left"
                },
                "config": {
                    "ai-cli": { "monitor_opencode": true },
                    "git": { "foo": 1 }
                }
            }
        });
        let changed = migrate(&mut raw);
        assert!(changed);
        let enabled = raw.pointer("/widgets/enabled").and_then(|v| v.as_array()).unwrap();
        assert!(enabled.iter().any(|v| v.as_str() == Some("ai_cli")));
        assert!(!enabled.iter().any(|v| v.as_str() == Some("ai-cli")));
        assert!(raw.pointer("/widgets/positions/ai_cli").is_some());
        assert!(raw.pointer("/widgets/positions/ai-cli").is_none());
        assert!(raw.pointer("/widgets/config/ai-cli").is_none());
    }

    /// Migration must be a no-op on already-migrated configs.
    #[test]
    fn migration_is_idempotent() {
        let mut raw = serde_json::json!({
            "widgets": {
                "enabled": ["workspace", "ai_cli"],
                "positions": { "ai_cli": "left" }
            }
        });
        let changed = migrate(&mut raw);
        assert!(!changed);
    }

    /// **Regression:** a previous buggy version of `migrate` did
    /// `widgets_obj.remove("config")` which wiped EVERY widget's saved
    /// settings (weather, system_stats, media, …) on every config load.
    /// This test proves unrelated widget-config sub-blocks survive the
    /// migration untouched.
    #[test]
    fn migration_preserves_unrelated_widget_config() {
        let mut raw = serde_json::json!({
            "widgets": {
                "enabled": ["ai-cli", "weather", "system_stats"],
                "positions": { "ai-cli": "left", "weather": "right" },
                "config": {
                    "ai-cli": { "monitor_opencode": true },
                    "weather": { "city": "Lisbon", "api_key": "secret" },
                    "system_stats": { "show_network": false },
                    "media": { "compact": true }
                }
            }
        });
        let _changed = migrate(&mut raw);

        // The ghost `ai-cli` config block must be gone…
        assert!(raw.pointer("/widgets/config/ai-cli").is_none(),
            "ai-cli config sub-block should be removed");
        // …but every other widget's config must survive untouched.
        assert_eq!(raw.pointer("/widgets/config/weather/city").and_then(|v| v.as_str()), Some("Lisbon"),
            "weather config must be preserved");
        assert_eq!(raw.pointer("/widgets/config/weather/api_key").and_then(|v| v.as_str()), Some("secret"),
            "weather api_key must be preserved");
        assert_eq!(raw.pointer("/widgets/config/system_stats/show_network").and_then(|v| v.as_bool()), Some(false),
            "system_stats.show_network=false must be preserved");
        assert_eq!(raw.pointer("/widgets/config/media/compact").and_then(|v| v.as_bool()), Some(true),
            "media.compact=true must be preserved");
    }
}
