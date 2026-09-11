use crate::{error::Result, state::AppState};
use remote_codex_client::application::*;
use serde::Deserialize;
use tauri::{State, WebviewWindow};

#[tauri::command]
pub(crate) async fn file_list(
    window: WebviewWindow,
    state: State<'_, AppState>,
    context: String,
    path: String,
) -> Result<DirectoryPage> {
    Ok(state.window(&window)?.files(&context)?.list(&path).await?)
}
#[tauri::command]
pub(crate) async fn file_read(
    window: WebviewWindow,
    state: State<'_, AppState>,
    context: String,
    path: String,
) -> Result<TextFile> {
    Ok(state.window(&window)?.files(&context)?.read(&path).await?)
}
#[tauri::command]
pub(crate) async fn file_write(
    window: WebviewWindow,
    state: State<'_, AppState>,
    context: String,
    path: String,
    text: String,
    revision: Option<String>,
) -> Result<String> {
    Ok(state
        .window(&window)?
        .files(&context)?
        .write(&path, &text, revision)
        .await?)
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum FileChange {
    Directory { path: String },
    Remove { path: String },
    Rename { path: String, destination: String },
}
#[tauri::command]
pub(crate) async fn file_change(
    window: WebviewWindow,
    state: State<'_, AppState>,
    context: String,
    change: FileChange,
) -> Result<()> {
    let handle = state.window(&window)?.files(&context)?;
    match change {
        FileChange::Directory { path } => handle.create_directory(&path).await?,
        FileChange::Remove { path } => handle.remove(&path).await?,
        FileChange::Rename { path, destination } => handle.rename(&path, &destination).await?,
    }
    Ok(())
}
#[tauri::command]
pub(crate) async fn project_mcp(
    window: WebviewWindow,
    state: State<'_, AppState>,
    workspace: String,
    servers: Option<Vec<ProjectMcpServer>>,
    revision: Option<String>,
) -> Result<ProjectMcpConfig> {
    let handle = state.window(&window)?.workspace(&workspace)?;
    Ok(if let Some(servers) = servers {
        handle.save_project_mcp(servers, revision).await?
    } else {
        handle.project_mcp().await?
    })
}
