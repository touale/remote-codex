use serde::Serialize;
#[derive(Debug, Serialize)]
pub(crate) struct Error {
    code: String,
    message: String,
    outcome_unknown: bool,
}
pub(crate) type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub(crate) fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            outcome_unknown: false,
        }
    }
}
impl From<remote_codex_client::ClientError> for Error {
    fn from(value: remote_codex_client::ClientError) -> Self {
        Self {
            code: value.code().into(),
            message: value.to_string(),
            outcome_unknown: value.outcome_is_unknown(),
        }
    }
}
impl From<tauri::Error> for Error {
    fn from(_: tauri::Error) -> Self {
        Self::new("WINDOW_ERROR", "The window is no longer available.")
    }
}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::new("IO_ERROR", "Cannot access local application preferences.")
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::new("INVALID_DATA", "Invalid application data.")
    }
}
