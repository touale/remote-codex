use crate::{
    error::Result,
    state::{AppState, Event, WindowState, operation, unavailable},
};
use remote_codex_client::application::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{State, WebviewWindow, ipc::Channel};

#[derive(Serialize)]
pub(crate) struct Catalog {
    servers: Vec<ServerSummary>,
    workspaces: Vec<Workspace>,
    sessions: Vec<CachedSession>,
    live: Vec<LiveSession>,
}
#[derive(Serialize)]
pub(crate) struct LiveSession {
    session: Session,
    server: String,
    settings: ConfirmedSettings,
}

#[tauri::command]
pub(crate) async fn attach(
    window: WebviewWindow,
    state: State<'_, AppState>,
    channel: Channel<Event>,
) -> Result<Option<(String, String)>> {
    let _gate = state.initialization.lock().await;
    if let Ok(context) = state.window(&window) {
        *context.events.lock().map_err(|_| unavailable())? = channel;
        return Ok(None);
    }
    let preferences = super::window::load_preferences(&window)?;
    let context =
        Arc::new(WindowState::new(channel, preferences.codex_program.map(Into::into)).await?);
    state
        .windows
        .lock()
        .map_err(|_| unavailable())?
        .insert(window.label().into(), context);
    Ok(state
        .startup_workspaces
        .lock()
        .map_err(|_| unavailable())?
        .remove(window.label()))
}
#[tauri::command]
pub(crate) async fn catalog(
    window: WebviewWindow,
    state: State<'_, AppState>,
    archived: bool,
) -> Result<Catalog> {
    let context = state.window(&window)?;
    let servers = context.client.servers().saved().await?;
    let workspaces = context.client.workspaces().list().await?;
    let sessions = context.client.sessions().list(None, archived).await?;
    let live = context
        .sessions
        .lock()
        .map_err(|_| unavailable())?
        .values()
        .map(|s| {
            Ok(LiveSession {
                session: s.session().clone(),
                server: s.server().into(),
                settings: s.current_settings()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Catalog {
        servers,
        workspaces,
        sessions,
        live,
    })
}

#[derive(Deserialize)]
pub(crate) struct ServerInput {
    name: String,
    address: String,
    port: Option<u16>,
    identity: Option<String>,
    password: Option<String>,
    settings: Vec<(String, String)>,
}
#[tauri::command]
pub(crate) async fn server_save(
    window: WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
    input: ServerInput,
) -> Result<ServerSummary> {
    let context = state.window(&window)?;
    let result = operation(&context, operation_id, async {
        Ok(context
            .client
            .servers()
            .add(AddServer {
                name: input.name,
                address: input.address,
                port: input.port,
                identity: input.identity.filter(|s| !s.is_empty()).map(Into::into),
                password: input
                    .password
                    .filter(|s| !s.is_empty())
                    .map(zeroize::Zeroizing::new),
                install_key: false,
                settings: input.settings,
            })
            .await?)
    })
    .await;
    state.changed();
    result
}
#[tauri::command]
pub(crate) async fn server_remove(
    window: WebviewWindow,
    state: State<'_, AppState>,
    name: String,
) -> Result<()> {
    let context = state.window(&window)?;
    context.close_files(Some(&name), None).await?;
    context
        .browsers
        .lock()
        .map_err(|_| unavailable())?
        .retain(|_, browser| {
            if browser.server() == name {
                browser.close();
                false
            } else {
                true
            }
        });
    context.client.servers().remove(&name, false).await?;
    state.changed();
    Ok(())
}
#[tauri::command]
pub(crate) async fn server_config(
    window: WebviewWindow,
    state: State<'_, AppState>,
    name: String,
    updates: Option<Vec<(String, String)>>,
    revision: Option<i64>,
) -> Result<remote_codex_client::config::ConfigReport> {
    let context = state.window(&window)?;
    if let Some(updates) = updates {
        context
            .client
            .config()
            .set_many(&name, &updates, revision.ok_or_else(unavailable)?)
            .await?;
        state.changed();
    }
    Ok(context.client.config().list(&name, false).await?)
}
#[tauri::command]
pub(crate) async fn workspace_open(
    window: WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
    server: String,
    path: String,
) -> Result<WorkspaceOpened> {
    let context = state.window(&window)?;
    if let Some((id, w)) = context
        .workspaces
        .lock()
        .map_err(|_| unavailable())?
        .iter()
        .find(|(_, w)| w.server() == server && w.path() == path)
    {
        return Ok(WorkspaceOpened {
            id: id.clone(),
            server: server.clone(),
            path: w.path().into(),
        });
    }
    let workspace = operation(&context, operation_id, async {
        Ok(context.client.workspaces().open(&server, &path).await?)
    })
    .await?;
    let mut workspaces = context.workspaces.lock().map_err(|_| unavailable())?;
    if context.closing.load(std::sync::atomic::Ordering::Acquire) {
        workspace.close();
        return Err(unavailable());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let result = WorkspaceOpened {
        id: id.clone(),
        path: workspace.path().into(),
        server,
    };
    workspaces.insert(id.clone(), workspace);
    drop(workspaces);
    state.changed();
    Ok(result)
}
#[derive(Serialize)]
pub(crate) struct WorkspaceOpened {
    id: String,
    server: String,
    path: String,
}
#[tauri::command]
pub(crate) async fn workspace_remove(
    window: WebviewWindow,
    state: State<'_, AppState>,
    server: String,
    path: String,
) -> Result<()> {
    let context = state.window(&window)?;
    context.close_files(Some(&server), Some(&path)).await?;
    context.client.workspaces().remove(&server, &path).await?;
    state.changed();
    Ok(())
}
#[tauri::command]
pub(crate) async fn workspace_close(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let context = state.window(&window)?;
    let workspace = context
        .workspaces
        .lock()
        .map_err(|_| unavailable())?
        .remove(&id);
    if let Some(workspace) = workspace {
        workspace.files().shutdown().await;
    }
    Ok(())
}
