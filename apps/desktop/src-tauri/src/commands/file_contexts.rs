use crate::{
    error::Result,
    state::{AppState, operation, unavailable},
};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{State, WebviewWindow};

#[derive(Serialize)]
pub(crate) struct FileContext {
    id: String,
    server: String,
    path: String,
    kind: &'static str,
}

/// Directory browsing does not register a saved workspace.
#[tauri::command]
pub(crate) async fn file_context_open(
    window: WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
    server: String,
    path: Option<String>,
) -> Result<FileContext> {
    let context = state.window(&window)?;
    let files = operation(&context, operation_id, async {
        Ok(context
            .client
            .servers()
            .files(&server, path.as_deref().unwrap_or("/"))
            .await?)
    })
    .await?;
    let id = uuid::Uuid::new_v4().to_string();
    let result = FileContext {
        id: id.clone(),
        server: files.server().into(),
        path: files.path().into(),
        kind: "server",
    };
    let mut contexts = context.file_contexts.lock().map_err(|_| unavailable())?;
    if context.closing.load(Ordering::Acquire) {
        files.close();
        return Err(unavailable());
    }
    contexts.insert(id, files);
    Ok(result)
}

#[tauri::command]
pub(crate) async fn file_context_close(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let context = state.window(&window)?;
    let files = context
        .file_contexts
        .lock()
        .map_err(|_| unavailable())?
        .remove(&id);
    if let Some(files) = files {
        files.shutdown().await;
    }
    Ok(())
}
