use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub appearance: AppearanceConfig,
    #[serde(default = "default_monitors")]
    pub monitors: MonitorsSelection,
    #[serde(default)]
    pub layout: LayoutConfig,
    #[serde(default)]
    pub widgets: WidgetsConfig,
    #[serde(default)]
    pub motion: MotionConfig,
    #[serde(default)]
    pub css: CssConfig,
    #[serde(default)]
    pub calendar_oauth: CalendarOauthConfig,
    #[serde(default)]
    pub updates: UpdatesConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

fn default_monitors() -> MonitorsSelection {
    MonitorsSelection::All
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MonitorsSelection {
    #[default]
    All,
    Only(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default)]
    pub background: BackgroundConfig,
    #[serde(default = "default_tint_alpha")]
    pub tint_alpha: u8,
    #[serde(default = "default_corner_radius")]
    pub corner_radius_tl: u32,
    #[serde(default = "default_corner_radius")]
    pub corner_radius_tr: u32,
    #[serde(default = "default_corner_radius")]
    pub corner_radius_br: u32,
    #[serde(default = "default_corner_radius")]
    pub corner_radius_bl: u32,
    #[serde(default)]
    pub margin_top: u32,
    #[serde(default)]
    pub margin_right: u32,
    #[serde(default)]
    pub margin_bottom: u32,
    #[serde(default)]
    pub margin_left: u32,
    #[serde(default)]
    pub padding_top: u32,
    #[serde(default = "default_padding_side")]
    pub padding_right: u32,
    #[serde(default)]
    pub padding_bottom: u32,
    #[serde(default = "default_padding_side")]
    pub padding_left: u32,
    #[serde(default = "default_bar_height")]
    pub bar_height: u32,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_tint_alpha() -> u8 {
    61
}
fn default_corner_radius() -> u32 {
    0
}
fn default_padding_side() -> u32 {
    8
}
fn default_bar_height() -> u32 {
    40
}
fn default_theme() -> String {
    "dark".into()
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            background: Default::default(),
            tint_alpha: default_tint_alpha(),
            corner_radius_tl: default_corner_radius(),
            corner_radius_tr: default_corner_radius(),
            corner_radius_br: default_corner_radius(),
            corner_radius_bl: default_corner_radius(),
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            padding_top: 0,
            padding_right: 8,
            padding_bottom: 0,
            padding_left: 8,
            bar_height: default_bar_height(),
            theme: default_theme(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundConfig {
    #[serde(default = "default_bg_mode")]
    pub mode: String,
    #[serde(default = "default_bg_color_top")]
    pub color_top: String,
    #[serde(default = "default_bg_color")]
    pub color_bottom: String,
    #[serde(default = "default_alpha_top")]
    pub alpha_top: u8,
    #[serde(default = "default_alpha_bottom")]
    pub alpha_bottom: u8,
}

fn default_bg_mode() -> String {
    "gradient".into()
}
fn default_bg_color() -> String {
    "#1a1a1a".into()
}
fn default_bg_color_top() -> String {
    "#1f2541".into()
}
fn default_alpha_top() -> u8 {
    60
}
fn default_alpha_bottom() -> u8 {
    0
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            mode: default_bg_mode(),
            color_top: default_bg_color_top(),
            color_bottom: default_bg_color(),
            alpha_top: default_alpha_top(),
            alpha_bottom: default_alpha_bottom(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    #[serde(default = "default_position")]
    pub position: String,
}

fn default_position() -> String {
    "top".into()
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            position: default_position(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetsConfig {
    #[serde(default)]
    pub enabled: Vec<String>,
    #[serde(default)]
    pub positions: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub config: std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
}

impl Default for WidgetsConfig {
    fn default() -> Self {
        let mut config = std::collections::HashMap::new();
        config.insert(
            "datetime".into(),
            std::collections::HashMap::from([
                ("show_events".into(), serde_json::Value::Bool(false)),
                ("show_date".into(), serde_json::Value::Bool(true)),
                ("show_year".into(), serde_json::Value::Bool(false)),
                ("show_next_month".into(), serde_json::Value::Bool(false)),
                ("format".into(), serde_json::Value::String("24h".into())),
                ("timezone".into(), serde_json::Value::String("".into())),
            ]),
        );
        config.insert(
            "alarms".into(),
            std::collections::HashMap::from([
                ("show_relative_time".into(), serde_json::Value::Bool(true)),
            ]),
        );
        config.insert(
            "media".into(),
            std::collections::HashMap::from([
                ("compact".into(), serde_json::Value::Bool(false)),
                ("scroll_label".into(), serde_json::Value::Bool(false)),
                ("thumb_style".into(), serde_json::Value::String("background".into())),
            ]),
        );
        config.insert(
            "links".into(),
            std::collections::HashMap::from([
                ("links".into(), serde_json::Value::Array(vec![])),
            ]),
        );
        config.insert(
            "system_stats".into(),
            std::collections::HashMap::from([
                ("style".into(), serde_json::Value::String("bar".into())),
                ("format".into(), serde_json::Value::String("percent".into())),
                ("history_size".into(), serde_json::Value::Number(20.into())),
                ("refresh_seconds".into(), serde_json::Value::Number(3.into())),
                ("show_cpu".into(), serde_json::Value::Bool(true)),
                ("show_ram".into(), serde_json::Value::Bool(true)),
                ("show_gpu".into(), serde_json::Value::Bool(true)),
                ("show_hd".into(), serde_json::Value::Bool(true)),
                ("show_network".into(), serde_json::Value::Bool(false)),
                ("selected_gpus".into(), serde_json::Value::Array(vec![serde_json::Value::String("GPU0".into())])),
                ("selected_hds".into(), serde_json::Value::Array(vec![serde_json::Value::String("C:".into())])),
                ("selected_networks".into(), serde_json::Value::Array(vec![serde_json::Value::String("Ethernet".into())])),
            ]),
        );
        config.insert(
            "quick_toggle".into(),
            std::collections::HashMap::from([
                ("compact".into(), serde_json::Value::Bool(true)),
                ("show_wifi".into(), serde_json::Value::Bool(true)),
                ("show_bluetooth".into(), serde_json::Value::Bool(true)),
                ("show_dark_mode".into(), serde_json::Value::Bool(true)),
                ("show_focus_assist".into(), serde_json::Value::Bool(true)),
                ("show_night_light".into(), serde_json::Value::Bool(true)),
                ("show_airplane".into(), serde_json::Value::Bool(false)),
            ]),
        );
        Self {
            enabled: vec![
                "workspace".into(),
                "media".into(),
                "links".into(),
                "datetime".into(),
                "alarms".into(),
                "system_stats".into(),
                "volume".into(),
                "shutdown".into(),
            ],
            positions: std::collections::HashMap::from([
                ("workspace".into(), "left".into()),
                ("media".into(), "left".into()),
                ("links".into(), "left".into()),
                ("datetime".into(), "center".into()),
                ("alarms".into(), "center".into()),
                ("system_stats".into(), "right".into()),
                ("volume".into(), "right".into()),
                ("shutdown".into(), "right".into()),
            ]),
            config,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotionConfig {
    #[serde(default = "default_motion_backend")]
    pub backend: String,
    #[serde(default)]
    pub reduced_motion: bool,
}

fn default_motion_backend() -> String {
    "auto".into()
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            backend: default_motion_backend(),
            reduced_motion: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CssConfig {
    #[serde(default = "default_custom_enabled")]
    pub custom_enabled: bool,
}

fn default_custom_enabled() -> bool {
    true
}

impl Default for CssConfig {
    fn default() -> Self {
        Self {
            custom_enabled: default_custom_enabled(),
        }
    }
}

/// User-supplied OAuth client ids for calendar providers. Empty strings fall
/// back to the shipped placeholder ids in `calendar_sync::credentials`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CalendarOauthConfig {
    #[serde(default)]
    pub google_client_id: String,
    #[serde(default)]
    pub outlook_client_id: String,
}

/// Update-checker settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatesConfig {
    /// When true, Zenith checks GitHub releases once every 24 hours and
    /// emits `zenith:update-available` when a newer version exists.
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    /// When true, Zenith launches automatically when the user signs in.
    #[serde(default = "default_start_with_windows")]
    pub start_with_windows: bool,
}

fn default_auto_update() -> bool {
    true
}

fn default_start_with_windows() -> bool {
    true
}

impl Default for UpdatesConfig {
    fn default() -> Self {
        Self {
            auto_update: default_auto_update(),
            start_with_windows: default_start_with_windows(),
        }
    }
}

/// Top-level toggle for OneDrive sync of Zenith data files. When enabled,
/// `config.json` and `calendar-events.json` are mirrored to
/// `<OneDrive>\Zenith\` on every save. Read by both the `config` domain
/// (config sync) and the `events` domain (events sync) via the shared
/// JSON pointer `/storage/onedrive_sync_enabled`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_onedrive_sync_enabled")]
    pub onedrive_sync_enabled: bool,
}

fn default_onedrive_sync_enabled() -> bool {
    false
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            onedrive_sync_enabled: default_onedrive_sync_enabled(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.appearance.background.mode, "gradient");
        assert_eq!(cfg.appearance.background.color_top, "#1f2541");
        assert_eq!(cfg.appearance.background.color_bottom, "#1a1a1a");
        assert_eq!(cfg.appearance.background.alpha_top, 60);
        assert_eq!(cfg.appearance.background.alpha_bottom, 0);
        assert_eq!(cfg.appearance.tint_alpha, 61);
        assert_eq!(cfg.appearance.bar_height, 40);
        assert_eq!(cfg.appearance.margin_bottom, 0);
        assert_eq!(cfg.layout.position, "top");
        assert_eq!(cfg.motion.backend, "auto");
        assert!(cfg.css.custom_enabled);
        assert_eq!(cfg.monitors, MonitorsSelection::All);
        assert!(!cfg.storage.onedrive_sync_enabled);
    }

    #[test]
    fn partial_json_fills_missing_with_defaults() {
        let raw = r#"{
            "appearance": { "background": { "mode": "mica" } },
            "widgets": { "enabled": ["datetime"] }
        }"#;
        let cfg: Config = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.appearance.background.mode, "mica");
        assert_eq!(cfg.appearance.bar_height, 40);
        assert_eq!(cfg.widgets.enabled, vec!["datetime".to_string()]);
        assert_eq!(cfg.layout.position, "top");
    }
}
