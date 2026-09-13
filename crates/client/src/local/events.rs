use super::{Generation, LocalRuntime};
use crate::{ClientError, Result};
use remote_codex_adapter::events::{self, Notice};
use remote_codex_core::session::{EnvironmentState, SessionEvent};
use remote_codex_protocol::Fault;
use serde_json::Value;
use std::sync::{Arc, Weak};

pub(super) fn start(runtime: &Arc<LocalRuntime>) -> Result<()> {
    let generation = runtime.current()?;
    runtime
        .intent
        .lock()
        .map_err(|_| ClientError::RemoteResponse)?
        .settings = generation.native.settings_snapshot();
    let owner = Arc::downgrade(runtime);
    let task = tokio::spawn(pump(
        owner,
        generation,
        runtime.lease.revoked.clone(),
        runtime.closed.subscribe(),
    ));
    let status = tokio::spawn(status(
        Arc::downgrade(runtime),
        runtime.recovery.state.subscribe(),
    ));
    runtime
        .event_task
        .lock()
        .map_err(|_| ClientError::RemoteResponse)?
        .extend([task, status]);
    Ok(())
}

async fn status(
    owner: Weak<LocalRuntime>,
    mut states: tokio::sync::watch::Receiver<EnvironmentState>,
) {
    let mut previous = EnvironmentState::Ready;
    while states.changed().await.is_ok() {
        let state = states.borrow_and_update().clone();
        let Some(runtime) = owner.upgrade() else {
            break;
        };
        let _ = runtime.events.send(SessionEvent::EnvironmentChanged {
            state: state.clone(),
        });
        match &state {
            EnvironmentState::Reconnecting {
                attempt,
                max_attempts,
                retry_in_ms,
            } => {
                if matches!(previous, EnvironmentState::Ready) {
                    if let Err(error) = runtime.suspend_goal().await {
                        runtime.announce(&format!("Could not suspend goal: {error}"));
                    }
                    if let Ok(mut intent) = runtime.intent.lock() {
                        intent.disconnect();
                    }
                }
                if !matches!(previous, EnvironmentState::Reconnecting { attempt: old, .. } if old == *attempt)
                {
                    let delay = if *retry_in_ms > 0 {
                        " · retry in 5s"
                    } else {
                        ""
                    };
                    runtime.announce(&format!(
                        "Connection lost. Reconnecting · attempt {attempt}/{max_attempts}{delay}"
                    ));
                }
            }
            EnvironmentState::Ready => {
                if !matches!(previous, EnvironmentState::Ready) {
                    runtime.announce("Execution connection restored.");
                    let _ = runtime.continue_interrupted(false).await;
                    if let Err(error) = runtime.restore_goal().await {
                        runtime.announce(&format!("Goal remains paused: {error}"));
                    }
                }
            }
            EnvironmentState::ActionRequired { message, .. } => runtime.announce(message),
            EnvironmentState::Closed => break,
            EnvironmentState::Recovering { .. } => {}
        }
        previous = state;
    }
}

async fn pump(
    owner: Weak<LocalRuntime>,
    mut generation: Arc<Generation>,
    mut revoked: tokio::sync::watch::Receiver<bool>,
    mut closed: tokio::sync::watch::Receiver<Option<Option<String>>>,
) {
    let result = async {
        let mut input = generation.native.take_events()?;
        let mut finished = generation.bridge.finished.clone();
        loop {
            let incoming = tokio::select! {
                event = input.recv() => Some(event.map_err(|_| ClientError::RemoteResponse)?),
                _ = finished.changed() => None,
                _ = revoked.changed() => return if *revoked.borrow() {Err(ClientError::Argument("session control moved to another frontend"))} else {Ok(())},
                _ = closed.changed() => return Ok(()),
            };
            let Some(runtime) = owner.upgrade() else { return Ok(()); };
            if runtime.closed.borrow().is_some() || *revoked.borrow() { return Ok(()); }
            let needs_rebuild = incoming.is_none() || (incoming.as_ref().is_some_and(|v| v["method"] == "remoteCodex/engineClosed") && runtime.recovery.ready().is_err());
            if needs_rebuild {
                let reason = finished.borrow().as_ref().map(|f| f.message.clone()).unwrap_or_else(|| "execution environment disconnected".into());
                generation = tokio::select! {
                    restored = runtime.rebuild(&generation, reason) => restored?,
                    _ = revoked.changed() => return Ok(()),
                    _ = closed.changed() => return Ok(()),
                };
                input = generation.native.take_events()?;
                finished = generation.bridge.finished.clone();
                runtime.continue_interrupted(true).await?;
                if !matches!(*runtime.recovery.state.borrow(), EnvironmentState::ActionRequired { .. }) { runtime.recovery.publish(EnvironmentState::Ready); }
                continue;
            }
            let Some(event) = incoming else { continue; };
            if !process(&runtime, &generation, event).await? { return Ok(()); }
        }
    }.await;
    if let Some(runtime) = owner.upgrade() {
        let reason = result.err().map(|error: ClientError| error.to_string());
        let _ = runtime.events.send(SessionEvent::Closed {
            reason: reason.clone(),
        });
        runtime.closed.send_replace(Some(reason));
        runtime.shutdown().await;
    }
}

async fn process(
    runtime: &LocalRuntime,
    generation: &Generation,
    mut event: Value,
) -> Result<bool> {
    if let Some(reply) = generation.native.goal_attachment_response(&event).await {
        generation.native.send(reply)?;
        return Ok(true);
    }
    if let Some(reply) = super::recovery::handle(runtime, &event).await {
        generation.native.send(reply)?;
        return Ok(true);
    }
    let notice = events::notice(&mut event, runtime.binding.execution_mode == "sandboxed")?;
    generation
        .approvals
        .observe(&notice, &runtime.binding, &generation.bridge.channel)?;
    let command_approval = matches!(notice, Notice::Approval(_));
    match notice {
        Notice::Permissions {
            thread,
            full_access,
        } => {
            generation.permissions.observe(&thread, full_access)?;
            runtime
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?
                .settings = event["params"]["threadSettings"].clone();
        }
        Notice::Closed(Some(error)) => return Err(error.into()),
        Notice::Closed(None) => return Ok(false),
        _ => {}
    }
    if let Some(id) = event.get("id") {
        let mut pending = generation
            .pending_requests
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?;
        if pending.len() >= 64 {
            return Err(ClientError::Argument("too many pending approvals"));
        }
        pending.insert(
            id.to_string(),
            super::generation::PendingRequest {
                event: event.clone(),
                command_approval,
            },
        );
    }
    {
        let mut intent = runtime
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?;
        if event["method"] == "turn/started" {
            intent.active = event["params"]["turn"]["id"].as_str().map(str::to_owned);
            intent.plan = None;
        }
        if event["method"] == "turn/completed" {
            intent.completed = event["params"]["turn"]["id"].as_str().map(str::to_owned);
            if intent.active == intent.completed {
                intent.active = None;
            }
            if event["params"]["turn"]["status"] == "completed" {
                intent.interrupted = None;
            }
        }
    }
    if event["method"] == "serverRequest/resolved" {
        let id = event["params"]["requestId"].to_string();
        generation
            .pending_requests
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .remove(&id);
    }
    let completed = event["method"] == "turn/completed";
    if let Some(public) = events::public_event(&event) {
        {
            let mut intent = runtime
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            intent.observe(&public);
        }
        let _ = runtime.events.send(public);
    }
    let summary_needed = completed || event_summary_needed(&event);
    let _ = runtime.native_events.send(event);
    if summary_needed
        && runtime.recovery.ready().is_ok()
        && let Err(error) = runtime.refresh_summary(generation).await
    {
        runtime.announce(&format!("Session title could not be refreshed: {error}"));
    }
    if completed && runtime.recovery.ready().is_ok() {
        runtime.continue_interrupted(false).await?;
        runtime.restore_goal().await?;
    }
    Ok(true)
}

impl LocalRuntime {
    pub(super) fn respond(&self, value: Value) -> Result<()> {
        if *self.lease.revoked.borrow() || self.closed.borrow().is_some() {
            return Err(
                Fault::new("SESSION_CLOSED", "session control is no longer available").into(),
            );
        }
        let id = value
            .get("id")
            .ok_or(ClientError::RemoteResponse)?
            .to_string();
        let generation = self.current()?;
        let request = generation
            .pending_requests
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .remove(&id)
            .ok_or_else(|| Fault::new("STALE_APPROVAL", "approval is no longer pending"))?;
        if request.command_approval {
            generation
                .approvals
                .respond(&id, events::decision(&value)?)?;
        }
        Ok(generation.native.send(value)?)
    }
}

fn event_summary_needed(event: &Value) -> bool {
    event["method"] == "thread/name/updated"
        || (event["method"] == "item/completed" && event["params"]["item"]["type"] == "userMessage")
}
