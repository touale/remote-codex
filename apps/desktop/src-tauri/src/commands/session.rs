use crate::{
    error::{Error, Result},
    state::{AppState, Event, operation, owned_elsewhere, unavailable},
};
use remote_codex_client::application::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Manager, State, WebviewWindow};

#[derive(Deserialize)]
pub(crate) struct SessionInput {
    server: String,
    path: String,
    resume: Option<String>,
    takeover: bool,
    mcp_source: Option<String>,
}
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum SessionOpened {
    Trust {
        preparation: String,
        server: String,
        path: String,
        names: Vec<String>,
    },
    Open {
        session: Session,
        settings: ConfirmedSettings,
        models: Vec<ModelOption>,
        snapshot: Box<SessionSnapshot>,
    },
}
#[tauri::command]
pub(crate) async fn session_open(
    window: WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
    input: SessionInput,
) -> Result<SessionOpened> {
    let context = state.window(&window)?;
    let permit = match &input.resume {
        Some(id) => Some(state.openings.acquire(&window, id).await?),
        None => None,
    };
    state.changed();
    if let Some(id) = &input.resume {
        if owned_elsewhere(window.app_handle(), window.label(), id)?.is_some() {
            return Err(Error::new(
                "SESSION_FOCUSED",
                "This session is open in another window.",
            ));
        }
        if let Ok(handle) = context.session(id) {
            return opened(&handle).await;
        }
    }
    let result = operation(&context, operation_id, async {
        let prepared = context
            .client
            .sessions()
            .prepare(OpenSession {
                server: input.server,
                path: input.path,
                resume: input.resume,
                takeover: input.takeover,
                mcp_source: input.mcp_source,
            })
            .await?;
        let trust = prepared.trust();
        if trust.required {
            let id = uuid::Uuid::new_v4().to_string();
            let mut pending = context.prepared.lock().map_err(|_| unavailable())?;
            if pending.len() >= 16 {
                return Err(Error::new(
                    "OPERATION_BUSY",
                    "Resolve pending project trust prompts first.",
                ));
            }
            pending.insert(
                id.clone(),
                crate::opening::PreparedOpen {
                    session: prepared,
                    _permit: permit,
                },
            );
            return Ok(SessionOpened::Trust {
                preparation: id,
                server: trust.server,
                path: trust.path,
                names: trust.names,
            });
        }
        register(
            &context,
            prepared.open(false).await?,
            window.app_handle().clone(),
        )
        .await
    })
    .await;
    state.changed();
    result
}
#[tauri::command]
pub(crate) async fn session_trust(
    window: WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
    preparation: String,
    accept: bool,
) -> Result<Option<SessionOpened>> {
    let context = state.window(&window)?;
    let prepared = context
        .prepared
        .lock()
        .map_err(|_| unavailable())?
        .remove(&preparation)
        .ok_or_else(unavailable)?;
    if !accept {
        drop(prepared);
        state.changed();
        return Ok(None);
    }
    let result = operation(&context, operation_id, async {
        Ok(Some(
            register(
                &context,
                prepared.session.open(true).await?,
                window.app_handle().clone(),
            )
            .await?,
        ))
    })
    .await;
    drop(prepared._permit);
    state.changed();
    result
}
async fn opened(handle: &SessionHandle) -> Result<SessionOpened> {
    Ok(SessionOpened::Open {
        session: handle.session().clone(),
        settings: handle.current_settings()?,
        models: handle.models().await?,
        snapshot: Box::new(handle.snapshot()?),
    })
}
async fn register(
    context: &Arc<crate::state::WindowState>,
    handle: SessionHandle,
    app: tauri::AppHandle,
) -> Result<SessionOpened> {
    let mut receiver = handle.events();
    let id = handle.session().id.clone();
    let result = match opened(&handle).await {
        Ok(result) => result,
        Err(error) => {
            handle.close().await;
            return Err(error);
        }
    };
    context
        .sessions
        .lock()
        .map_err(|_| unavailable())?
        .insert(id.clone(), handle);
    let owner = Arc::downgrade(context);
    let stream_id = id.clone();
    let events_app = app.clone();
    let task = tokio::spawn(async move {
        loop {
            let received = receiver.recv().await;
            let Some(context) = owner.upgrade() else {
                break;
            };
            match received {
                Ok(event) => {
                    if matches!(event, SessionEvent::SessionUpdated { .. }) {
                        events_app.state::<AppState>().changed();
                    }
                    let closed = matches!(event, SessionEvent::Closed { .. });
                    if !context
                        .deliver(Event::Session {
                            id: id.clone(),
                            event,
                        })
                        .await
                    {
                        break;
                    }
                    if closed {
                        if let Ok(mut sessions) = context.sessions.lock() {
                            sessions.remove(&id);
                        }
                        events_app.state::<AppState>().changed();
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    receiver = match context.session(&id) {
                        Ok(handle) => handle.events(),
                        Err(_) => break,
                    };
                    if !context.deliver(Event::Resync { id: id.clone() }).await {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    context
        .streams
        .lock()
        .map_err(|_| unavailable())?
        .insert(stream_id, task.abort_handle());
    app.state::<AppState>().changed();
    Ok(result)
}
