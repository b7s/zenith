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
pub const EVENT_APPEARANCE_CHANGED: &str = "zenith:appearance-changed";
pub const EVENT_WIDGETS_CHANGED: &str = "zenith:widgets-changed";
pub const EVENT_WORKSPACE_CHANGED: &str = "zenith:workspace-changed";
