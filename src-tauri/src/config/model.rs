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
}

fn default_monitors() -> MonitorsSelection {
    MonitorsSelection::All
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MonitorsSelection {
    All,
    Only(Vec<String>),
}

impl Default for MonitorsSelection {
    fn default() -> Self {
        MonitorsSelection::All
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default = "default_material")]
    pub material: String,
    #[serde(default = "default_tint_alpha")]
    pub tint_alpha: u8,
    #[serde(default)]
    pub background: BackgroundConfig,
    #[serde(default = "default_corner_radius")]
    pub corner_radius: u32,
    #[serde(default)]
    pub margin_top: i32,
    #[serde(default)]
    pub margin_left: i32,
    #[serde(default)]
    pub margin_right: i32,
    #[serde(default = "default_bar_height")]
    pub bar_height: u32,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_material() -> String {
    "acrylic".into()
}
fn default_tint_alpha() -> u8 {
    60
}
fn default_corner_radius() -> u32 {
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
            material: default_material(),
            tint_alpha: default_tint_alpha(),
            background: Default::default(),
            corner_radius: default_corner_radius(),
            margin_top: 0,
            margin_left: 0,
            margin_right: 0,
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
    #[serde(default = "default_gradient_direction")]
    pub gradient_direction: String,
    #[serde(default = "default_alpha")]
    pub alpha_top: u8,
    #[serde(default = "default_alpha")]
    pub alpha_bottom: u8,
}

fn default_bg_mode() -> String {
    "transparent".into()
}
fn default_bg_color() -> String {
    "#1a1a1a".into()
}
fn default_gradient_direction() -> String {
    "to_bottom".into()
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
            gradient_direction: default_gradient_direction(),
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WidgetsConfig {
    #[serde(default)]
    pub enabled: Vec<String>,
    #[serde(default)]
    pub positions: std::collections::HashMap<String, String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.appearance.material, "acrylic");
        assert_eq!(cfg.appearance.bar_height, 40);
        assert_eq!(cfg.appearance.background.mode, "transparent");
        assert_eq!(cfg.layout.position, "top");
        assert_eq!(cfg.motion.backend, "auto");
        assert!(cfg.css.custom_enabled);
        assert_eq!(cfg.monitors, MonitorsSelection::All);
    }

    #[test]
    fn partial_json_fills_missing_with_defaults() {
        let raw = r#"{
            "appearance": { "material": "mica" },
            "widgets": { "enabled": ["clock"] }
        }"#;
        let cfg: Config = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.appearance.material, "mica");
        assert_eq!(cfg.appearance.bar_height, 40);
        assert_eq!(cfg.widgets.enabled, vec!["clock".to_string()]);
        assert_eq!(cfg.layout.position, "top");
    }
}
