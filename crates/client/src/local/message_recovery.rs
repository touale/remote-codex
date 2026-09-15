use super::{ClientError, EnvironmentState, LocalRuntime, Result};
use remote_codex_protocol::Fault;

/// Owns only a message that has not reached Codex yet. Dropping its request
/// releases the slot; neither a reconnect nor a later Retry can submit it.
pub(super) struct PendingMessage<'a> {
    runtime: &'a LocalRuntime,
}

impl Drop for PendingMessage<'_> {
    fn drop(&mut self) {
        if let Ok(mut intent) = self.runtime.intent.lock() {
            intent.pending_message = None;
        }
    }
}

impl LocalRuntime {
    /// Native TUI input waits for turn/start to return. Start recovery without
    /// holding that request or queuing a message behind the disconnected backend.
    pub(super) fn retry_for_message(&self) -> Result<()> {
        if !matches!(&*self.recovery.state.borrow(), EnvironmentState::ActionRequired { code, .. } if code == "RECOVERY_RETRIES_EXHAUSTED")
        {
            return self.recovery.ready();
        }
        {
            let mut intent = self
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            intent.interrupted = None;
            intent.episode = None;
            intent.resume_goal = None;
            intent.resend_after_recovery = true;
        }
        self.recovery.retry();
        Err(Fault::new(
            "ENVIRONMENT_NOT_READY",
            "Reconnecting. This message was not submitted. Send it again when the connection is restored.",
        )
        .into())
    }

    pub(super) async fn recover_for_message(&self) -> Result<Option<PendingMessage<'_>>> {
        let mut states = self.recovery.state.subscribe();
        if !matches!(&*states.borrow(), EnvironmentState::ActionRequired { code, .. } if code == "RECOVERY_RETRIES_EXHAUSTED")
        {
            self.recovery.ready()?;
            return Ok(None);
        }
        let (cancel, mut cancelled) = tokio::sync::watch::channel(false);
        {
            let mut intent = self
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            if intent.pending_message.is_some() {
                return Err(Fault::new(
                    "MESSAGE_PENDING",
                    "A message is already waiting for the connection.",
                )
                .into());
            }
            intent.pending_message = Some(cancel);
            // The user's new instruction replaces automatic continuation, even
            // when reconnecting this time also fails.
            intent.interrupted = None;
            intent.episode = None;
            intent.resume_goal = None;
        }
        let pending = PendingMessage { runtime: self };
        let mut closed = self.closed.subscribe();
        let mut revoked = self.lease.revoked.clone();
        if closed.borrow().is_some() || *revoked.borrow() {
            return Err(
                Fault::new("SESSION_CLOSED", "session control is no longer available").into(),
            );
        }
        self.recovery.retry();
        loop {
            tokio::select! {
                changed = states.changed() => {
                    changed.map_err(|_| ClientError::RemoteResponse)?;
                    match states.borrow_and_update().clone() {
                        EnvironmentState::Ready => return Ok(Some(pending)),
                        EnvironmentState::ActionRequired { code, message } => return Err(Fault::new(&code, &format!("{message} This message was not submitted.")).into()),
                        EnvironmentState::Closed => return Err(Fault::new("SESSION_CLOSED", "session closed; this message was not submitted").into()),
                        _ => {},
                    }
                }
                _ = cancelled.changed() => return Err(Fault::new("MESSAGE_CANCELLED", "Message cancelled before submission.").into()),
                _ = closed.changed() => return Err(Fault::new("SESSION_CLOSED", "session closed; this message was not submitted").into()),
                _ = revoked.changed() => return Err(Fault::new("LEASE_REVOKED", "session control moved; this message was not submitted").into()),
            }
        }
    }
}
