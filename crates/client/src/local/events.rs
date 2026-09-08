use super::LocalRuntime;
use crate::{ClientError, Result};
use remote_codex_adapter::events::{self, Notice};
use remote_codex_core::session::SessionEvent;
use remote_codex_protocol::Fault;
use serde_json::Value;
use std::sync::{Arc, Weak};

pub(super) fn start(runtime: &Arc<LocalRuntime>) {
    let events = runtime.native.subscribe();
    let owner = Arc::downgrade(runtime);
    let task = tokio::spawn(pump(owner, events, runtime.lease.revoked.clone()));
    if let Ok(mut slot) = runtime.event_task.lock() {
        *slot = Some(task);
    }
}

async fn pump(
    owner: Weak<LocalRuntime>,
    mut input: tokio::sync::broadcast::Receiver<Value>,
    mut revoked: tokio::sync::watch::Receiver<bool>,
) {
    let result = async {
        loop {
            let mut event = tokio::select! {
                event = input.recv() => event.map_err(|_| ClientError::RemoteResponse)?,
                _ = revoked.changed() => {
                    return if *revoked.borrow() {Err(ClientError::Argument("session control moved to another frontend"))} else {Ok(())};
                },
            };
            let Some(runtime) = owner.upgrade() else {
                return Ok(());
            };
            if let Some(reply) = super::recovery::handle(&runtime, &event).await {
                runtime.native.send(reply)?;
                continue;
            }
            let notice = events::notice(&mut event, runtime.binding.execution_mode == "sandboxed")?;
            runtime
                .approvals
                .observe(&notice, &runtime.binding, &runtime.bridge.channel)?;
            let command_approval = matches!(notice, Notice::Approval(_));
            match notice {
                Notice::Permissions {
                    thread,
                    full_access,
                } => runtime.permissions.observe(&thread, full_access)?,
                Notice::Closed(Some(error)) => return Err(error.into()),
                Notice::Closed(None) => return Ok(()),
                _ => {}
            }
            if let Some(id) = event.get("id") {
                let mut pending = runtime
                    .pending_approvals
                    .lock()
                    .map_err(|_| ClientError::RemoteResponse)?;
                if pending.len() >= 64 {
                    return Err(ClientError::Argument("too many pending approvals"));
                }
                pending.insert(id.to_string(), command_approval);
            }
            if let Some(public) = events::public_event(&event) {
                let _ = runtime.events.send(public);
            }
            let _ = runtime.native_events.send(event);
        }
    }
    .await;
    if let Some(runtime) = owner.upgrade() {
        let reason = result.err().map(|error: ClientError| error.to_string());
        let _ = runtime.events.send(SessionEvent::Closed {
            reason: reason.clone(),
        });
        runtime.closed.send_replace(Some(reason));
        runtime.shutdown().await;
    }
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
        let command = self
            .pending_approvals
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .remove(&id)
            .ok_or_else(|| Fault::new("STALE_APPROVAL", "approval is no longer pending"))?;
        if command {
            self.approvals.respond(&id, events::decision(&value)?)?;
        }
        Ok(self.native.send(value)?)
    }
}
