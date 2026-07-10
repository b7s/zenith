pub mod commands;
pub mod listen;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub thumbnail: Option<String>,
    pub status: String,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub rate: f64,
    pub source: String,
}
