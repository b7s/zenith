use serde::{Deserialize, Serialize};

/// Mirrored in `src/shared/types.ts` as `CalendarEvent`.
///
/// Every field carries `#[serde(default)]` (AGENTS.md §5) so a partial
/// JSON object from the frontend (e.g. `add_event` without `id` /
/// `created_at` / `updated_at`) always deserialises — the `add_event`
/// command fills in the missing pieces after deserialisation.
///
/// External-calendar sync (Google Calendar / Outlook) writes events
/// into the **same** store with `source` set to `"google"` / `"outlook"`
/// (empty string for locally-created events) and `external_id` set to
/// the provider's stable event id. The `id` of an external event is
/// derived from `"{source}:{external_id}"` so subsequent syncs can
/// upsert the same row without duplicating.
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
    /// Optional end time (`"HH:MM"`) for synced calendar events. Lets the
    /// alarm popup + alarms widget show "until HH:MM". Empty for local
    /// alarms (which are instantaneous).
    #[serde(default)]
    pub end_time: Option<String>,
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
    /// Origin of the event — empty for user-created, `"google"` or
    /// `"outlook"` for synced entries. Used by the sync domain to
    /// pick the right adapter on subsequent updates.
    #[serde(default)]
    pub source: String,
    /// Internal id of the `CalendarAccount` that sourced this event.
    /// Empty for local events. Lets the bar/alarms widget show the
    /// account badge without reaching into config.
    #[serde(default)]
    pub source_account_id: String,
    /// Stable provider-side identifier (Google event id / Outlook
    /// event id). Empty for local events. Sync uses this to diff
    /// against the API on the next round.
    #[serde(default)]
    pub external_id: String,
    /// When true (the default for synced events), the alarm-fire
    /// thread raises the popup notification when this event's
    /// `date`+`time` arrives. Recurring events fire once per
    /// occurrence. Local one-shot alarms are unaffected.
    #[serde(default = "default_true")]
    pub notify_on_start: bool,
    /// Epoch-seconds of the last time the start notification fired for
    /// this row. Lets the alarm-fire thread skip rows that already
    /// fired this occurrence. Empty after one-shot completion.
    #[serde(default)]
    pub last_notified_at: i64,
}

fn default_true() -> bool {
    true
}

/// Sentinel sources (also used by the sync domain). Kept as constants
/// rather than an enum so the on-disk JSON is a plain string and older
/// files tolerate unknown values.
pub mod source {
    pub const LOCAL: &str = "";
    pub const GOOGLE: &str = "google";
    pub const OUTLOOK: &str = "outlook";
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
