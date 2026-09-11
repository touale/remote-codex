use super::{LocalRuntime, intent::Intent};
use crate::{ClientError, Result};
use remote_codex_core::{desktop::RevertedSession, goals::GoalStatus};
use remote_codex_protocol::Fault;

impl LocalRuntime {
    pub(crate) async fn revert(&self, before_turn: &str) -> Result<RevertedSession> {
        let _goal = self.goal_gate.lock().await;
        let _turn = self.turn_gate.lock().await;
        if self.closed.borrow().is_some() || *self.lease.revoked.borrow() {
            return Err(
                Fault::new("SESSION_CLOSED", "Session control is no longer available.").into(),
            );
        }
        self.recovery.ready()?;
        let generation = self.current()?;
        generation.bridge.check()?;
        {
            let intent = self
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            if intent.active.is_some()
                || intent
                    .goal
                    .as_ref()
                    .is_some_and(|g| g.status == GoalStatus::Active)
            {
                return Err(Fault::new(
                    "TURN_ACTIVE",
                    "Stop the task and pause its goal before editing a message.",
                )
                .into());
            }
        }
        if before_turn.is_empty() {
            return Err(ClientError::Argument("Select the message to edit."));
        }
        generation.native.revert(before_turn).await?;
        // The native operation has committed. Failures below require a history
        // refresh, never an automatic second revert or message submission.
        let refreshed = async {
            {
                let mut intent = self
                    .intent
                    .lock()
                    .map_err(|_| ClientError::RemoteResponse)?;
                let mut status = intent.status.clone();
                status.usage = None;
                let settings = intent.settings.clone();
                let goal = intent.goal.clone();
                *intent = Intent::with_status(status);
                intent.settings = settings;
                intent.goal = goal;
            }

            let mut history = generation.native.history().await?;
            self.store.message_times(&mut history).await?;
            self.refresh_summary(&generation).await?;
            Ok::<_, ClientError>(RevertedSession {
                history,
                snapshot: self.snapshot()?,
            })
        }
        .await;
        refreshed.map_err(|e| Fault::unknown(&format!("History was reverted, but could not be refreshed: {e}. Reload this session before continuing.")).into())
    }
}
