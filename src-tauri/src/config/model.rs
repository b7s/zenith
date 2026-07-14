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
    102
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
    "auto".into()
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
    #[serde(default = "default_bg_color")]
    pub color_top: String,
    #[serde(default = "default_bg_color")]
    pub color_bottom: String,
    #[serde(default = "default_alpha")]
    pub alpha_top: u8,
    #[serde(default = "default_alpha")]
    pub alpha_bottom: u8,
}

fn default_bg_mode() -> String {
    "acrylic".into()
}
fn default_bg_color() -> String {
    "#1a1a1a".into()
}
fn default_alpha() -> u8 {
    100
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            mode: default_bg_mode(),
            color_top: default_bg_color(),
            color_bottom: default_bg_color(),
            alpha_top: default_alpha(),
            alpha_bottom: default_alpha(),
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
        Self {
            enabled: vec!["workspace".into(), "clock".into(), "volume".into()],
            positions: std::collections::HashMap::from([
                ("clock".into(), "center".into()),
                ("workspace".into(), "left".into()),
                ("volume".into(), "right".into()),
            ]),
            config: std::collections::HashMap::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.appearance.background.mode, "acrylic");
        assert_eq!(cfg.appearance.bar_height, 40);
        assert_eq!(cfg.appearance.margin_bottom, 0);
        assert_eq!(cfg.layout.position, "top");
        assert_eq!(cfg.motion.backend, "auto");
        assert!(cfg.css.custom_enabled);
        assert_eq!(cfg.monitors, MonitorsSelection::All);
    }

    #[test]
    fn partial_json_fills_missing_with_defaults() {
        let raw = r#"{
            "appearance": { "background": { "mode": "mica" } },
            "widgets": { "enabled": ["clock"] }
        }"#;
        let cfg: Config = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.appearance.background.mode, "mica");
        assert_eq!(cfg.appearance.bar_height, 40);
        assert_eq!(cfg.widgets.enabled, vec!["clock".to_string()]);
        assert_eq!(cfg.layout.position, "top");
    }
}
