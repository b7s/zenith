use serde::{Deserialize, Serialize};

/// Mirrored in `src/shared/types.ts` as `CalendarEvent`.
///
/// Every field carries `#[serde(default)]` (AGENTS.md §5) so a partial
/// JSON object from the frontend (e.g. `add_event` without `id` /
/// `created_at` / `updated_at`) always deserialises — the `add_event`
/// command fills in the missing pieces after deserialisation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalendarEvent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub kind: EventKind,
    #[serde(default)]
    pub recurrence: Recurrence,
    #[serde(default)]
    pub weekdays: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub notes: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Event,
    Alarm,
}

impl Default for EventKind {
    fn default() -> Self {
        EventKind::Event
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Recurrence {
    None,
    Daily,
    Weekly,
    Monthly,
}

impl Default for Recurrence {
    fn default() -> Self {
        Recurrence::None
    }
}

impl EventKind {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::Event => "event",
            EventKind::Alarm => "alarm",
        }
    }
}
