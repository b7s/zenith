//! DTOs for the ai-cli widget domain.
//! Mirrored in `src/shared/types.ts` — keep both in sync.

use serde::{Deserialize, Serialize};

/// IDs of supported AI coding CLIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CliId {
    Opencode,
    Claude,
    Codex,
}

impl CliId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Opencode => "opencode",
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "opencode" => Some(Self::Opencode),
            "claude" | "claude code" => Some(Self::Claude),
            "codex" | "codex cli" => Some(Self::Codex),
            _ => None,
        }
    }
}

/// Aggregate state for the bar widget — three independent booleans drive the
/// three dots (red = unseen failure, blue = running, green = finished/idle).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AggregateState {
    #[serde(default)]
    pub has_unseen_failure: bool,
    #[serde(default)]
    pub any_running: bool,
    #[serde(default)]
    pub any_finished: bool,
    #[serde(default)]
    pub per_cli: Vec<CliSnapshot>,
}

/// Per-CLI snapshot — shown in the manager window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliSnapshot {
    pub cli_id: String,
    pub is_running: bool,
    /// CLI is waiting for user confirmation (permission prompt, question, etc.)
    #[serde(default)]
    pub is_waiting: bool,
    pub last_error_at: Option<i64>,
    pub last_error_message: String,
    pub current_prompt_label: String,
    /// Human-readable status like "idle", "running: fix auth bug"
    pub status_text: String,
}

/// Result of a CLI detection probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliDetected {
    pub cli_id: String,
    pub installed: bool,
    pub binary_path: String,
    pub config_dir: String,
    pub version: String,
}

/// An error session that has not yet been acknowledged by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnseenFailure {
    pub cli_id: CliId,
    pub error_message: String,
    pub occurred_at: i64,
    pub prompt_label: String,
}

/// Session event received from a CLI hook or SSE.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliEvent {
    pub cli_id: CliId,
    pub event_type: CliEventType,
    pub prompt_label: Option<String>,
    pub error_message: Option<String>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CliEventType {
    /// Session became active / started prompting.
    Started,
    /// Session finished a turn and is idle.
    Idle,
    /// Session completed normally.
    Completed,
    /// Session encountered an error.
    Failed,
    /// Session is waiting for user confirmation (permission, question).
    Waiting,
}
