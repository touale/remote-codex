use super::{Generation, LocalRuntime, intent::Intent};
use crate::{ClientError, Result};
use remote_codex_core::{desktop::RevertedSession, goals::GoalStatus};
use remote_codex_protocol::Fault;
use serde_json::Value;

impl LocalRuntime {
    pub(crate) async fn revert(&self, before_turn: &str) -> Result<RevertedSession> {
        let _goal = self.goal_gate.lock().await;
        let _turn = self.turn_gate.lock().await;
        let generation = self.current()?;
        self.revert_locked(&generation, before_turn).await?;
        let refreshed = async {
            let mut history = generation.native.history().await?;
            self.store.message_times(&mut history).await?;
            Ok(RevertedSession {
                history,
                snapshot: self.snapshot()?,
            })
        }
        .await;
        refreshed.map_err(refresh_error)
    }

    pub(crate) async fn revert_native(&self, before_turn: &str) -> Result<Value> {
        let _goal = self.goal_gate.lock().await;
        let _turn = self.turn_gate.lock().await;
        let generation = self.current()?;
        self.revert_locked(&generation, before_turn).await
    }

    // The caller holds the goal and turn gates through validation and state refresh.
    async fn revert_locked(&self, generation: &Generation, before_turn: &str) -> Result<Value> {
        if self.closed.borrow().is_some() || *self.lease.revoked.borrow() {
            return Err(
                Fault::new("SESSION_CLOSED", "Session control is no longer available.").into(),
            );
        }
        self.recovery.ready()?;
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
        let response = generation.native.revert(before_turn).await?;
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

            self.refresh_summary(generation).await
        }
        .await;
        refreshed.map_err(refresh_error)?;
        Ok(response)
    }
}

fn refresh_error(error: ClientError) -> ClientError {
    Fault::unknown(&format!("History was reverted, but could not be refreshed: {error}. Reload this session before continuing.")).into()
}
