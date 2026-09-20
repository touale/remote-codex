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
    #[serde(flatten)]
    pub content: FileContent,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum FileContent {
    Text {
        text: String,
        original: String,
        revision: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        view: Option<EditorView>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mode: Option<MarkdownMode>,
    },
    Preview {
        #[serde(rename = "previewView", skip_serializing_if = "Option::is_none")]
        preview_view: Option<PreviewView>,
    },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MarkdownMode {
    Edit,
    Preview,
    Split,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum PreviewScale {
    Number(f64),
    Fit(FitScale),
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) enum FitScale {
    #[serde(rename = "fit")]
    Fit,
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct PreviewView {
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scale: Option<PreviewScale>,
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
        || matches!(&file.content, FileContent::Text { text, original, .. }
            if text.len() > 4 * 1024 * 1024 || original.len() > 4 * 1024 * 1024)
        || matches!(&file.content, FileContent::Preview { preview_view: Some(view) }
            if matches!(view.scale, Some(PreviewScale::Number(scale)) if !scale.is_finite() || !(0.1..=10.0).contains(&scale)))
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
                if let FileContent::Text {
                    text,
                    original,
                    revision,
                    view,
                    ..
                } = &mut document.content
                {
                    text.clear();
                    original.clear();
                    revision.clear();
                    *view = None;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_preview_paths_and_view_without_text_payloads() -> Result<()> {
        let value = json!({"server":"dev", "root":"/project", "path":"paper.pdf",
            "kind":"preview", "previewView":{"page":2,"scale":"fit"}});
        let document: FileDocument = serde_json::from_value(value.clone())?;
        check_file(&document)?;
        assert_eq!(serde_json::to_value(&document)?, value);
        for (field, replacement) in [
            ("path", json!("../private.pdf")),
            ("previewView", json!({"scale": -1})),
        ] {
            let mut invalid = value.clone();
            invalid[field] = replacement;
            assert!(check_file(&serde_json::from_value(invalid)?).is_err());
        }
        Ok(())
    }
}
