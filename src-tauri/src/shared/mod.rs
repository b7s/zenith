use std::fmt;

#[derive(Debug)]
pub struct AppError(pub String);

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError(e.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError(s)
    }
}

pub type AppResult<T> = Result<T, AppError>;

pub const EVENT_CONFIG_UPDATED: &str = "zenith:config-updated";
#[allow(dead_code)]
pub const EVENT_APPEARANCE_CHANGED: &str = "zenith:appearance-changed";
#[allow(dead_code)]
pub const EVENT_WIDGETS_CHANGED: &str = "zenith:widgets-changed";
#[allow(dead_code)]
pub const EVENT_WORKSPACE_CHANGED: &str = "zenith:workspace-changed";
pub const EVENT_EVENTS_UPDATED: &str = "zenith:events-updated";
pub const EVENT_MEDIA_CHANGED: &str = "zenith:media-changed";
/// Emitted by the git poll thread when total failed-CI / open-PR counts
/// change across the user's configured GitHub/GitLab/Bitbucket accounts.
/// Payload is the full `GitState` so the frontend doesn't need to
/// round-trip back via `get_git_state`.
#[allow(dead_code)]
pub const EVENT_GIT_CHANGED: &str = "zenith:git-changed";
/// Emitted to the calendar popup window to switch its view mode
/// (`"calendar"` | `"events"`) when the window is reused across
/// callers (e.g. datetime widget → 2-month grid, then alarms widget →
/// events list). On a fresh open the view is seeded via the
/// `__ZENITH_CALENDAR_VIEW` init script; this event handles the
/// reuse case where the init script does not re-run.
pub const EVENT_CALENDAR_VIEW: &str = "zenith:calendar-view";

pub mod known_folders;
pub mod shell;

use std::sync::OnceLock;
use tauri::AppHandle;

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Install the global `AppHandle` during `lib.rs::setup` so detached
/// background threads (OAuth callbacks, calendar poll) can emit events
/// without threading an `AppHandle` through every call site.
pub fn set_app_handle(handle: AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

/// Borrow the global `AppHandle` if it was installed. Returns `None` if
/// called before setup completes (e.g. during early boot).
pub fn app_handle() -> Option<AppHandle> {
    APP_HANDLE.get().cloned()
}
