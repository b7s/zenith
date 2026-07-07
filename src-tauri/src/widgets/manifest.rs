use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetManifest {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_zone")]
    pub default_zone: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default = "default_min_width")]
    pub min_width: u32,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub config: IndexMap<String, WidgetConfigField>,
    #[serde(skip)]
    pub widget_dir: String,
}

fn default_zone() -> String {
    "left".into()
}

fn default_min_width() -> u32 {
    40
}

impl Default for WidgetManifest {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            version: String::new(),
            description: String::new(),
            default_zone: default_zone(),
            icon: String::new(),
            min_width: default_min_width(),
            preview: String::new(),
            config: IndexMap::new(),
            widget_dir: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetConfigField {
    #[serde(default, rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub value: serde_json::Value,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub hint: String,
    #[serde(default)]
    pub options: Vec<serde_json::Value>,
}
