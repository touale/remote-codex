use crate::{
    error::Result,
    state::{AppState, Event, operation, unavailable},
};
use remote_codex_client::application::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{State, WebviewWindow};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum TerminalTarget {
    Server { server: String },
    Files { context: String },
}
#[derive(Serialize)]
pub(crate) struct OpenedTerminal {
    id: String,
    server: String,
    path: String,
}

#[tauri::command]
pub(crate) async fn terminal_open(
    window: WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
    target: TerminalTarget,
    columns: u16,
    rows: u16,
) -> Result<OpenedTerminal> {
    let context = state.window(&window)?;
    let terminal = Arc::new(
        operation(&context, operation_id, async {
            Ok(match target {
                TerminalTarget::Files { context: id } => {
                    context.files(&id)?.terminal(columns, rows).await?
                }
                TerminalTarget::Server { server } => {
                    context
                        .client
                        .servers()
                        .terminal(&server, columns, rows)
                        .await?
                }
            })
        })
        .await?,
    );
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut terminals = context.terminals.lock().map_err(|_| unavailable())?;
        if context.closing.load(std::sync::atomic::Ordering::Acquire) {
            terminal.close();
            return Err(unavailable());
        }
        terminals.insert(id.clone(), terminal.clone());
    }
    let result = OpenedTerminal {
        id: id.clone(),
        server: terminal.server().into(),
        path: terminal.initial_path().into(),
    };
    let stream_id = id.clone();
    let owner = Arc::downgrade(&context);
    let task = tokio::spawn(async move {
        while let Some(event) = terminal.next().await {
            let ended = matches!(event, ShellEvent::Closed { .. });
            if let Some(context) = owner.upgrade() {
                if !context
                    .deliver(Event::Terminal {
                        id: stream_id.clone(),
                        event,
                    })
                    .await
                {
                    break;
                }
            } else {
                break;
            }
            if ended {
                break;
            }
        }
        terminal.close();
    });
    context
        .streams
        .lock()
        .map_err(|_| unavailable())?
        .insert(id.clone(), task.abort_handle());
    Ok(result)
}
#[tauri::command]
pub(crate) async fn terminal_input(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    bytes: Vec<u8>,
) -> Result<()> {
    Ok(state.window(&window)?.terminal(&id)?.write(&bytes)?)
}
#[tauri::command]
pub(crate) async fn terminal_resize(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    columns: u16,
    rows: u16,
) -> Result<()> {
    Ok(state
        .window(&window)?
        .terminal(&id)?
        .resize(columns, rows)?)
}
#[tauri::command]
pub(crate) async fn terminal_close(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let context = state.window(&window)?;
    context.close_stream(&id);
    if let Some(handle) = context
        .terminals
        .lock()
        .map_err(|_| unavailable())?
        .remove(&id)
    {
        handle.close();
    }
    Ok(())
}
