use crate::{
    error::Result,
    state::{AppState, operation, unavailable},
};
use remote_codex_client::application::{DirectoryBrowser, DirectoryPage};
use std::sync::atomic::Ordering;
use tauri::{State, WebviewWindow};

#[tauri::command]
pub(crate) async fn directory_open(
    window: WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
    server: String,
) -> Result<String> {
    let context = state.window(&window)?;
    let browser = operation(&context, operation_id, async {
        Ok(context.client.workspaces().browser(&server).await?)
    })
    .await?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut browsers = context.browsers.lock().map_err(|_| unavailable())?;
    if context.closing.load(Ordering::Acquire) {
        browser.close();
        return Err(unavailable());
    }
    browsers.insert(id.clone(), browser);
    Ok(id)
}

fn owned(window: &WebviewWindow, state: &AppState, id: &str) -> Result<DirectoryBrowser> {
    state
        .window(window)?
        .browsers
        .lock()
        .map_err(|_| unavailable())?
        .get(id)
        .cloned()
        .ok_or_else(unavailable)
}

#[tauri::command]
pub(crate) async fn directory_browse(
    window: WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
    browser: String,
    path: String,
) -> Result<DirectoryPage> {
    let context = state.window(&window)?;
    let handle = owned(&window, &state, &browser)?;
    operation(&context, operation_id, async {
        Ok(handle.list(&path).await?)
    })
    .await
}

#[tauri::command]
pub(crate) async fn directory_create(
    window: WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
    browser: String,
    parent: String,
    name: String,
) -> Result<String> {
    let context = state.window(&window)?;
    let handle = owned(&window, &state, &browser)?;
    operation(&context, operation_id, async {
        Ok(handle.create_directory(&parent, &name).await?)
    })
    .await
}

#[tauri::command]
pub(crate) async fn directory_close(
    window: WebviewWindow,
    state: State<'_, AppState>,
    browser: String,
) -> Result<()> {
    let context = state.window(&window)?;
    let handle = context
        .browsers
        .lock()
        .map_err(|_| unavailable())?
        .remove(&browser)
        .ok_or_else(unavailable)?;
    handle.close();
    Ok(())
}
