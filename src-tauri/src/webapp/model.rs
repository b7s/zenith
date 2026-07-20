use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Header {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkItem {
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub persistent: bool,
    /// Deprecated. Icons now live on disk at `<APPDATA>\zenith\icons\<id>.png`
    /// (see `webapp::icons`). This field exists only so the one-time
    /// `migrate_legacy_data_urls()` startup hook can move any old `data:`
    /// URL out of config.json and onto disk. New configs always have this
    /// set to `None`.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub headers: Vec<Header>,
    /// Saved window position (OS pixels). `None` = position from bar-anchor.
    #[serde(default)]
    pub pos_x: Option<i32>,
    /// Saved window position (OS pixels). `None` = position from bar-anchor.
    #[serde(default)]
    pub pos_y: Option<i32>,
}

impl Default for LinkItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            enabled: true,
            label: String::new(),
            url: String::new(),
            width: default_width(),
            height: default_height(),
            persistent: false,
            icon: None,
            headers: Vec::new(),
            pos_x: None,
            pos_y: None,
        }
    }
}

fn default_true() -> bool { true }
fn default_width() -> u32 { 1000 }
fn default_height() -> u32 { 700 }

pub fn load_links() -> Vec<LinkItem> {
    let cfg = crate::config::load();
    let wc = &cfg.widgets.config;
    let link_map = match wc.get("links") {
        Some(m) => m,
        None => return Vec::new(),
    };
    match link_map.get("links") {
        Some(v) => serde_json::from_value(v.clone()).unwrap_or_default(),
        None => Vec::new(),
    }
}

pub fn find_link(id: &str) -> Option<LinkItem> {
    load_links().into_iter().find(|a| a.id == id)
}
