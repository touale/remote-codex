use crate::{
    error::Result,
    state::{AppState, unavailable},
};
use remote_codex_client::application::*;
use serde::Deserialize;
use tauri::{State, WebviewWindow};
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum SessionAction {
    Submit {
        text: String,
        client_id: Option<String>,
    },
    Steer {
        text: String,
        client_id: String,
        turn: String,
    },
    Interrupt {
        turn: String,
    },
    Settings {
        settings: SessionSettings,
    },
    Approve {
        request: String,
        decision: ApprovalDecision,
    },
    Interact {
        request: String,
        answer: InteractionAnswer,
    },
    Goal {
        goal: GoalAction,
    },
    Retry,
}
#[tauri::command]
pub(crate) async fn session_action(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    action: SessionAction,
) -> Result<Option<Submission>> {
    let context = state.window(&window)?;
    let handle = context.session(&id)?;
    let result = match action {
        SessionAction::Submit { text, client_id } => Some(
            handle
                .message(
                    text,
                    client_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    None,
                )
                .await?,
        ),
        SessionAction::Steer {
            text,
            client_id,
            turn,
        } => Some(handle.message(text, client_id, Some(turn)).await?),
        SessionAction::Interrupt { turn } => {
            handle.interrupt(&turn).await?;
            None
        }
        SessionAction::Settings { settings } => {
            handle.settings(settings).await?;
            None
        }
        SessionAction::Approve { request, decision } => {
            handle.approve(&request, decision)?;
            None
        }
        SessionAction::Interact { request, answer } => {
            handle.interact(&request, answer)?;
            None
        }
        SessionAction::Goal { goal } => {
            handle.goal(goal).await?;
            None
        }
        SessionAction::Retry => {
            handle.retry();
            None
        }
    };
    state.changed();
    Ok(result)
}
#[tauri::command]
pub(crate) async fn session_history(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    cursor: Option<String>,
) -> Result<HistoryPage> {
    Ok(state
        .window(&window)?
        .client
        .sessions()
        .read(&id, cursor.as_deref())
        .await?)
}
#[tauri::command]
pub(crate) async fn session_tool_output(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    source: ToolOutputSource,
    offset: usize,
) -> Result<ToolOutputChunk> {
    Ok(state
        .window(&window)?
        .session(&id)?
        .tool_output(source, offset)
        .await?)
}

#[tauri::command]
pub(crate) async fn session_close(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let context = state.window(&window)?;
    context.close_stream(&id);
    let handle = context
        .sessions
        .lock()
        .map_err(|_| unavailable())?
        .remove(&id);
    if let Some(handle) = handle {
        handle.close().await;
    }
    state.changed();
    Ok(())
}
#[tauri::command]
pub(crate) async fn session_metadata(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    archived: Option<bool>,
) -> Result<()> {
    state
        .window(&window)?
        .client
        .sessions()
        .update_metadata(&id, name, archived)
        .await?;
    state.changed();
    Ok(())
}
#[tauri::command]
pub(crate) async fn session_snapshot(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<SessionSnapshot> {
    Ok(state.window(&window)?.session(&id)?.snapshot()?)
}
#[tauri::command]
pub(crate) async fn session_mcp(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<McpStatus>> {
    Ok(state.window(&window)?.session(&id)?.mcp_status().await?)
}

#[tauri::command]
pub(crate) async fn session_status(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<SessionSnapshot> {
    Ok(state.window(&window)?.session(&id)?.status().await?)
}

#[tauri::command]
pub(crate) async fn session_revert(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    before_turn_id: String,
) -> Result<RevertedSession> {
    let result = state
        .window(&window)?
        .session(&id)?
        .revert(&before_turn_id)
        .await?;
    state.changed();
    Ok(result)
}
