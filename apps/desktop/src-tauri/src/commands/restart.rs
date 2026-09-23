use crate::{
    error::Result,
    state::{AppState, Event, unavailable},
};
use remote_codex_client::application::{GoalStatus, UpdateComponent, Updater};
use serde::Serialize;
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use tauri::{State, WebviewWindow};

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum RestartResult {
    ConfirmationRequired {
        sessions: usize,
        terminals: usize,
        transfers: usize,
    },
    Closing,
}

#[tauri::command]
pub(crate) async fn restart_app(
    window: WebviewWindow,
    state: State<'_, AppState>,
    force: bool,
) -> Result<RestartResult> {
    let _gate = state.initialization.lock().await;
    state.window(&window)?;
    if state.restarting.load(Ordering::Acquire) {
        return Ok(RestartResult::Closing);
    }
    if Updater::open(None, UpdateComponent::App)?
        .snapshot()
        .await?
        .busy
    {
        return Err(crate::error::Error::new(
            "UPDATE_BUSY",
            "An app update is running. Wait for it to finish before restarting.",
        ));
    }
    let windows: Vec<_> = state
        .windows
        .lock()
        .map_err(|_| unavailable())?
        .values()
        .cloned()
        .collect();
    if !force {
        let mut sessions = HashSet::new();
        let mut terminals = 0;
        let mut transfers = HashSet::new();
        for context in &windows {
            for session in context.sessions.lock().map_err(|_| unavailable())?.values() {
                let snapshot = session.snapshot()?;
                if !snapshot.closed
                    && (snapshot.status.activity == "active"
                        || snapshot.turn.is_some()
                        || !snapshot.pending.is_empty()
                        || snapshot
                            .goal
                            .as_ref()
                            .is_some_and(|goal| goal.status == GoalStatus::Active))
                {
                    sessions.insert(session.session().id.clone());
                }
            }
            terminals += context
                .terminals
                .lock()
                .map_err(|_| unavailable())?
                .values()
                .filter(|terminal| !terminal.is_closed())
                .count();
            transfers.extend(context.client.transfers().running_ids()?);
        }
        if !sessions.is_empty() || terminals > 0 || !transfers.is_empty() {
            return Ok(RestartResult::ConfirmationRequired {
                sessions: sessions.len(),
                terminals,
                transfers: transfers.len(),
            });
        }
    }
    state.restarting.store(true, Ordering::Release);
    for context in windows {
        context.send(Event::CloseRequested);
    }
    Ok(RestartResult::Closing)
}
