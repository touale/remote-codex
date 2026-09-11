use crate::{
    error::Result,
    state::{AppState, Event},
};
use remote_codex_client::application::{SkippedTransfers, Transfer, TransferChoice};
use std::sync::Arc;
use tauri::{Manager, State, WebviewWindow};
#[tauri::command]
pub(crate) async fn transfer_list(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<Transfer>> {
    Ok(state.window(&window)?.client.transfers().list().await?)
}
#[tauri::command]
pub(crate) async fn transfer_run(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let context = state.window(&window)?;
    let app = window.app_handle().clone();
    let progress = Arc::new(move |transfer: Transfer| {
        let state = app.state::<AppState>();
        if let Ok(windows) = state.windows.lock() {
            for context in windows.values() {
                let mut transfer = transfer.clone();
                transfer.owned =
                    transfer.active && context.client.transfers().is_running(&transfer.id);
                context.send(Event::Transfer { transfer });
            }
        }
    });
    Ok(context.client.transfers().run(&id, progress).await?)
}
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum Action {
    Pause,
    Cancel,
    Restart,
    Resolve { choice: TransferChoice, all: bool },
}
#[tauri::command]
pub(crate) async fn transfer_action(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    action: Action,
) -> Result<()> {
    let service = state.window(&window)?.client.transfers();
    match action {
        Action::Pause => service.pause(&id).await?,
        Action::Cancel => service.cancel(&id).await?,
        Action::Restart => service.restart_file(&id).await?,
        Action::Resolve { choice, all } => service.resolve(&id, choice, all).await?,
    }
    Ok(())
}
#[tauri::command]
pub(crate) async fn transfer_skipped(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    after: Option<i64>,
) -> Result<SkippedTransfers> {
    Ok(state
        .window(&window)?
        .client
        .transfers()
        .skipped(&id, after)
        .await?)
}
#[tauri::command]
pub(crate) async fn transfer_reveal(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let path = state
        .window(&window)?
        .client
        .transfers()
        .download_directory(&id)
        .await?;
    tokio::process::Command::new("/usr/bin/open")
        .arg("--")
        .arg(path)
        .status()
        .await?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn transfer_remove(
    window: WebviewWindow,
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<Vec<String>> {
    let removed = state
        .window(&window)?
        .client
        .transfers()
        .remove_finished(&ids)
        .await?;
    if !removed.is_empty() {
        state.broadcast(Event::TransfersRemoved {
            ids: removed.clone(),
        });
    }
    Ok(removed)
}
