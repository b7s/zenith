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

impl Default for CalendarAccountProvider {
    fn default() -> Self {
        CalendarAccountProvider::Google
    }
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
