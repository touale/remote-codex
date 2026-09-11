use crate::{error::Result, state::AppState};
use remote_codex_client::application::{LoginStart, NativeStatus};
use tauri::{State, WebviewWindow};

#[tauri::command]
pub(crate) async fn native_status(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<NativeStatus> {
    let context = state.window(&window)?;
    let mut native = context.native.lock().await;
    if native.is_none() {
        *native = Some(context.client.native().open().await?);
    }
    Ok(native
        .as_ref()
        .ok_or_else(crate::state::unavailable)?
        .status()
        .await?)
}
#[tauri::command]
pub(crate) async fn native_login(
    window: WebviewWindow,
    state: State<'_, AppState>,
    cancel_id: Option<String>,
) -> Result<Option<LoginStart>> {
    let context = state.window(&window)?;
    let mut native = context.native.lock().await;
    if native.is_none() {
        *native = Some(context.client.native().open().await?);
    }
    let account = native.as_ref().ok_or_else(crate::state::unavailable)?;
    if let Some(id) = cancel_id {
        account.cancel_login(&id).await?;
        Ok(None)
    } else {
        Ok(Some(account.login().await?))
    }
}
#[tauri::command]
pub(crate) async fn native_select(
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
) -> Result<NativeStatus> {
    let context = state.window(&window)?;
    context.client.select_program(path.clone().into()).await?;
    let mut native = context.native.lock().await;
    if let Some(previous) = native.take() {
        previous.close().await;
    }
    let selected = context.client.native().open().await?;
    let status = selected.status().await?;
    *native = Some(selected);
    drop(native);
    let mut settings = super::window::load_preferences(&window)?;
    settings.codex_program = Some(path);
    super::window::preferences(window, state, Some(settings)).await?;
    Ok(status)
}

#[tauri::command]
pub(crate) async fn native_usage(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<remote_codex_client::application::AccountUsage> {
    let context = state.window(&window)?;
    let mut native = context.native.lock().await;
    if native.is_none() {
        *native = Some(context.client.native().open().await?);
    }
    Ok(native
        .as_ref()
        .ok_or_else(crate::state::unavailable)?
        .usage()
        .await?)
}
#[tauri::command]
pub(crate) async fn session_defaults(
    window: WebviewWindow,
    state: State<'_, AppState>,
    server: String,
) -> Result<remote_codex_client::application::SessionDefaults> {
    let context = state.window(&window)?;
    let mut native = context.native.lock().await;
    if native.is_none() {
        *native = Some(context.client.native().open().await?);
    }
    Ok(context
        .client
        .native()
        .defaults(
            native.as_ref().ok_or_else(crate::state::unavailable)?,
            &server,
        )
        .await?)
}
