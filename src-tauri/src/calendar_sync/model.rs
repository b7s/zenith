//! Calendar account + OAuth state shared by the calendar_sync domain.
//!
//! Mirrored in `src/shared/types.ts`:
//!   * `CalendarAccount`       ↔ `CalendarAccount`
//!   * `CalendarAccountProvider` ↔ `CalendarAccountProvider`

use serde::{Deserialize, Serialize};

/// All the data we keep for a connected Google Calendar / Outlook account.
/// Tokens are stored as **base64(Dpapi-protected ciphertext)** — plain
/// text only ever lives in process memory between `protect()` and the
/// in-flight HTTP request. See `git::secrets` for the DPAPI helpers
/// (single home — reused here, not reimplemented).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CalendarAccount {
    /// Internal stable id (`UUIDv4`). Used as the `source_account_id`
    /// on every event this account produces.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub provider: CalendarAccountProvider,
    /// User-facing label (e.g. "Work Calendar").
    #[serde(default)]
    pub label: String,
    /// Account email Google/Microsoft reported during sign-in.
    #[serde(default)]
    pub account_email: String,
    /// base64(DPAPI(access_token)). Short-lived (≤ 1h).
    #[serde(default)]
    pub access_token_blob: String,
    /// base64(DPAPI(refresh_token)). Long-lived.
    #[serde(default)]
    pub refresh_token_blob: String,
    /// Epoch-seconds when `access_token_blob` is considered expired.
    /// Refresh is needed after this point.
    #[serde(default)]
    pub expires_at: i64,
    /// Per-account poll interval in minutes. Default 15. The shared
    /// poll thread skips accounts whose cadence hasn't elapsed.
    #[serde(default = "default_poll_mins")]
    pub poll_mins: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Epoch-seconds of the last successful sync; 0 if never.
    #[serde(default)]
    pub last_sync_at: i64,
    /// Last error (empty when healthy). Surfaced to the UI so the
    /// user can decide whether to reconnect.
    #[serde(default)]
    pub last_error: String,
}

fn default_true() -> bool {
    true
}
fn default_poll_mins() -> u32 {
    15
}

/// Provider discriminator. Kept as a real enum (not a string) so the
/// sync engine dispatches on it in one place.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CalendarAccountProvider {
    Google,
    Outlook,
}

impl CalendarAccountProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            CalendarAccountProvider::Google => "google",
            CalendarAccountProvider::Outlook => "outlook",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "google" => Some(CalendarAccountProvider::Google),
            "outlook" => Some(CalendarAccountProvider::Outlook),
            _ => None,
        }
    }
}

impl Default for CalendarAccountProvider {
    fn default() -> Self {
        CalendarAccountProvider::Google
    }
}

/// One in-flight OAuth flow. The frontend polls
/// `poll_pending_auth(pending_id)` to learn when the loopback callback
/// completes (success → account was saved to config; failure → error
/// message). The pending state is held in a `Mutex<HashMap>` inside
/// the oauth module and removed when the flow resolves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAuth {
    /// Auth provider (google/outlook) — purely informational here; the
    /// stored `code_verifier`/endpoint selection live in the oauth
    /// module keyed by `pending_id`.
    pub provider: String,
    /// Echo'd `state` we sent to the authorize URL. The callback
    /// handler refuses any redirect that doesn't match — guards
    /// against CSRF and concurrent flows stealing each other's code.
    pub state: String,
    /// Loopback port we bound the local HTTP server on. The authorize
    /// URL embeds it (`http://127.0.0.1:<port>/callback`).
    pub port: u16,
    /// Unix-seconds when the flow was opened. The oauth module
    /// auto-expires stale flows after 5 minutes so abandoned clicks
    /// don't squat on a port forever.
    pub opened_at: i64,
}

/// Shape returned by `poll_pending_auth`. The frontend should re-fetch
/// config on success (the account is already there).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state")]
pub enum PendingAuthStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "ok")]
    Ok { account_id: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "expired")]
    Expired,
}
