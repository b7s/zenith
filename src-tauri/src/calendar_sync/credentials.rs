//! OAuth client credentials shipped with the binary.
//!
//! **These are placeholders.** Before publishing, the maintainer must
//! register Zenith as a desktop OAuth client in Google Cloud Console +
//! Microsoft Entra (Azure) and replace the constants below:
//!
//! * Google: `https://console.cloud.google.com/apis/credentials` →
//!   Create OAuth client ID → Application type **Desktop app**.
//!   Add Authorized redirect URI `http://127.0.0.1:0/callback`
//!   (we use `127.0.0.1:<random-ephemeral-port>` so any port the OS
//!   binds to is valid). Enable the Google Calendar API on the
//!   project.
//!
//! * Microsoft: `https://entra.microsoft.com/#view/Microsoft_AAD_RegisteredApps`
//!   → New registration → Mobile and desktop applications → Redirect URI
//!   `http://127.0.0.1` (Microsoft Graph accepts a bare host when the
//!   port comes from the system; we still pass
//!   `http://127.0.0.1:<port>` to satisfy the validator). Add the
//!   delegated permission `Calendars.Read` under Microsoft Graph.
//!
//! We use the **PKCE / public client** flow so no client secret is
//! embedded — the secrets here can be safely committed to a public
//! repo. Long-lived refresh tokens are protected with DPAPI before
//! they ever leave this process.

pub mod google {
    /// Google Calendar OAuth client id. Public (PKCE) — safe to ship.
    pub const CLIENT_ID: &str = "ZENITH_GOOGLE_CLIENT_ID_PLACEHOLDER";
    /// Google Calendar scopes. Read-only: list events only, never mutate.
    pub const SCOPES: &str = "https://www.googleapis.com/auth/calendar.readonly";
    /// Google authorize endpoint (browser-facing).
    pub const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
    /// Google token-exchange endpoint.
    pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
    /// Google Calendar read endpoint — events list for the primary calendar.
    pub const EVENTS_URL: &str =
        "https://www.googleapis.com/calendar/v3/calendars/primary/events";
    /// Userinfo endpoint used during connect to capture the email address.
    pub const USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";
    /// Stable source tag stored on synced events.
    pub const SOURCE: &str = "google";
}

pub mod outlook {
    /// Microsoft Graph OAuth client id. Public (PKCE) — safe to ship.
    pub const CLIENT_ID: &str = "ZENITH_OUTLOOK_CLIENT_ID_PLACEHOLDER";
    /// `offline_access` is what returns a refresh token. `Calendars.Read`
    /// is what grants event-list access.
    pub const SCOPES: &str = "offline_access https://graph.microsoft.com/Calendars.Read";
    /// Microsoft identity-platform authorize endpoint (multi-tenant).
    pub const AUTHORIZE_URL: &str =
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
    /// Token-exchange endpoint.
    pub const TOKEN_URL: &str =
        "https://login.microsoftonline.com/common/oauth2/v2.0/token";
    /// Microsoft Graph calendar-view endpoint for the signed-in user.
    pub const EVENTS_URL: &str = "https://graph.microsoft.com/v1.0/me/calendarview";
    /// Graph `/me` endpoint used during connect to capture the email/UPN.
    pub const USERINFO_URL: &str = "https://graph.microsoft.com/v1.0/me";
    /// Stable source tag stored on synced events.
    pub const SOURCE: &str = "outlook";
}
