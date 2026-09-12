use crate::{
    error::{Error, Result},
    state::{AppState, unavailable},
};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State, WebviewWindow};
use tokio::sync::oneshot;

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct EditorView {
    line: u32,
    column: u32,
    top: f64,
    left: f64,
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct FileDocument {
    pub server: String,
    pub root: String,
    pub path: String,
    pub text: String,
    pub original: String,
    pub revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<EditorView>,
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct ChangeDocument {
    pub server: String,
    pub root: String,
    pub path: String,
    pub diff: String,
}
pub(crate) struct FileTransfer {
    pub owner: String,
    pub id: String,
    pub complete: oneshot::Sender<Result<()>>,
}
pub(super) fn check_file(file: &FileDocument) -> Result<()> {
    if file.path.is_empty()
        || file
            .path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
        || file.path.chars().any(char::is_control)
        || file.text.len() > 4 * 1024 * 1024
        || file.original.len() > 4 * 1024 * 1024
    {
        return Err(Error::new(
            "INVALID_FILE",
            "Cannot transfer this file document.",
        ));
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn editor_window_ready(
    window: WebviewWindow,
    state: State<'_, AppState>,
    error: Option<String>,
) -> Result<()> {
    state.window(&window)?;
    let pending = state
        .file_transfers
        .lock()
        .map_err(|_| unavailable())?
        .remove(window.label());
    let pending = pending
        .ok_or_else(|| Error::new("TRANSFER_CANCELLED", "This file transfer was cancelled."))?;
    let outcome = match error {
        Some(message) => Err(Error::new("FILE_WINDOW_FAILED", &message)),
        None => {
            if let Some(super::window::WindowTarget::File {
                transfer, document, ..
            }) = state
                .startup_targets
                .lock()
                .map_err(|_| unavailable())?
                .get_mut(window.label())
            {
                transfer.clear();
                document.text.clear();
                document.original.clear();
                document.revision.clear();
                document.view = None;
            }
            Ok(())
        }
    };
    pending.complete.send(outcome).map_err(|_| unavailable())
}
#[tauri::command]
pub(crate) async fn editor_window_cancel(
    window: WebviewWindow,
    state: State<'_, AppState>,
    transfer: String,
) -> Result<bool> {
    state.window(&window)?;
    let mut transfers = state.file_transfers.lock().map_err(|_| unavailable())?;
    let label = transfers
        .iter()
        .find(|(_, p)| p.owner == window.label() && p.id == transfer)
        .map(|(label, _)| label.clone());
    if let Some(label) = label {
        if let Some(pending) = transfers.remove(&label) {
            let _ = pending.complete.send(Err(cancelled()));
        }
        return Ok(true);
    }
    Ok(false)
}
pub(super) fn cancel_for(state: &AppState, label: &str) -> Result<()> {
    let mut transfers = state.file_transfers.lock().map_err(|_| unavailable())?;
    let labels: Vec<_> = transfers
        .iter()
        .filter(|(target, pending)| *target == label || pending.owner == label)
        .map(|(label, _)| label.clone())
        .collect();
    for label in labels {
        if let Some(pending) = transfers.remove(&label) {
            let _ = pending.complete.send(Err(cancelled()));
        }
    }
    Ok(())
}
fn cancelled() -> Error {
    Error::new(
        "TRANSFER_CANCELLED",
        "File window opening was cancelled. Your original tab is kept.",
    )
}
pub(super) async fn discard(window: &WebviewWindow, state: &AppState, label: &str) {
    if let Ok(mut targets) = state.startup_targets.lock() {
        targets.remove(label);
    }
    let context = state
        .windows
        .lock()
        .ok()
        .and_then(|windows| windows.get(label).cloned());
    if let Some(context) = context {
        if context.closing.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        context.close().await;
        if let Ok(mut windows) = state.windows.lock() {
            windows.remove(label);
        }
    }
    if let Some(child) = window.app_handle().get_webview_window(label) {
        let _ = child.destroy();
    }
}
