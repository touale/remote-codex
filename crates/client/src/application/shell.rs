#[cfg(unix)]
#[path = "shell_unix.rs"]
mod native;
#[cfg(windows)]
#[path = "shell_windows.rs"]
mod native;
pub use native::ShellHandle;
pub(super) use native::open;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellEvent {
    Output { bytes: Vec<u8> },
    Closed { code: Option<u32> },
}
