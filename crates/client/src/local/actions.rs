use super::{LocalRuntime, route};
use crate::Result;
use remote_codex_adapter::{
    events,
    thread::action::{Action, turn_id},
};
use remote_codex_core::session::{ApprovalDecision, SessionEvent, SessionSettings};
use std::{ffi::OsString, path::Path, sync::atomic::Ordering};

impl LocalRuntime {
    pub(crate) fn snapshot(&self) -> Result<remote_codex_core::desktop::SessionSnapshot> {
        let generation = self.current()?;
        let intent = self
            .intent
            .lock()
            .map_err(|_| crate::ClientError::RemoteResponse)?;
        let pending = generation
            .pending_requests
            .lock()
            .map_err(|_| crate::ClientError::RemoteResponse)?
            .values()
            .filter_map(|request| events::public_event(&request.event))
            .collect();
        Ok(remote_codex_core::desktop::SessionSnapshot {
            status: intent.status.clone(),
            current_turn: intent.current_turn.clone(),
            goal: intent.goal.clone(),
            plan: intent.plan.clone(),
            settings: remote_codex_adapter::desktop::settings(&intent.settings),
            environment: self.recovery.state.borrow().clone(),
            turn: intent.active.clone(),
            pending,
            closed: self.closed.borrow().is_some(),
        })
    }
    pub(crate) fn interact(
        &self,
        id: &str,
        answer: remote_codex_core::desktop::InteractionAnswer,
    ) -> Result<()> {
        let generation = self.current()?;
        let request = generation
            .pending_requests
            .lock()
            .map_err(|_| crate::ClientError::RemoteResponse)?
            .get(id)
            .map(|request| request.event.clone())
            .ok_or(crate::ClientError::Argument(
                "This interaction is no longer pending.",
            ))?;
        self.respond(remote_codex_adapter::interactions::answer(
            &request, answer,
        )?)
    }
    pub(crate) fn current_settings(&self) -> Result<remote_codex_core::desktop::ConfirmedSettings> {
        let intent = self
            .intent
            .lock()
            .map_err(|_| crate::ClientError::RemoteResponse)?;
        Ok(remote_codex_adapter::desktop::settings(&intent.settings))
    }
    pub(crate) async fn models(&self) -> Result<Vec<remote_codex_core::desktop::ModelOption>> {
        Ok(self.current()?.native.models().await?)
    }
    pub(crate) async fn mcp_status(&self) -> Result<Vec<remote_codex_core::desktop::McpStatus>> {
        Ok(self.current()?.native.mcp_status().await?)
    }
    pub(super) async fn action(&self, action: Action) -> Result<serde_json::Value> {
        let _pending = if matches!(
            &action,
            Action::Message {
                expected_turn: None,
                ..
            }
        ) {
            self.recover_for_message().await?
        } else {
            None
        };
        let generation = self.current()?;
        let prepared = generation.native.prepare_action(
            action,
            self.has_history.load(Ordering::Acquire),
            generation.skills.mappings(),
        )?;
        Ok(route::execute(self, &generation, prepared).await?)
    }

    pub(crate) fn events(&self) -> tokio::sync::broadcast::Receiver<SessionEvent> {
        self.events.subscribe()
    }

    pub(super) async fn submit_recovery(
        &self,
        text: String,
        interrupted_turn: &str,
    ) -> Result<String> {
        let generation = self.current()?;
        let prepared = generation.native.prepare_action(
            Action::Submit(text),
            self.has_history.load(Ordering::Acquire),
            generation.skills.mappings(),
        )?;
        Ok(turn_id(
            &route::execute_recovery(self, &generation, prepared, interrupted_turn).await?,
        )?)
    }

    pub(crate) async fn message(
        &self,
        text: String,
        client_id: String,
        expected_turn: Option<String>,
    ) -> Result<remote_codex_core::status::Submission> {
        let response = self
            .action(Action::Message {
                text,
                client_id: client_id.clone(),
                expected_turn,
            })
            .await?;
        let turn_id = match response["turnId"].as_str() {
            Some(turn) => turn.to_owned(),
            None => turn_id(&response)?,
        };
        let sent_at = match self
            .store
            .message_time(&self.binding.session.id, &client_id)
            .await
        {
            Ok(value) => value,
            Err(_) => {
                // Submission already succeeded. A display metadata read must never invite replay.
                let _ = self.events.send(SessionEvent::Warning {
                    message: "Message submitted; its timestamp is temporarily unavailable.".into(),
                });
                None
            }
        };
        Ok(remote_codex_core::status::Submission { turn_id, sent_at })
    }

    pub(crate) async fn settings(&self, settings: SessionSettings) -> Result<()> {
        if settings.mode.is_some() {
            self.change_mode(settings).await?;
        } else {
            let _gate = self.goal_gate.lock().await;
            self.action(Action::Settings(settings)).await?;
        }
        Ok(())
    }

    pub(crate) async fn interrupt(&self, turn: &str) -> Result<()> {
        if self.closed.borrow().is_some() || *self.lease.revoked.borrow() {
            return Err(remote_codex_protocol::Fault::new(
                "SESSION_CLOSED",
                "session control is no longer available",
            )
            .into());
        }
        self.cancel_continuation(turn)?;
        if self.recovery.ready().is_err() {
            let _ = self.current()?.native.queue_interrupt(turn);
            return Ok(());
        }
        self.pause_goal().await?;
        self.action(Action::Interrupt(turn.into())).await?;
        Ok(())
    }

    pub(crate) fn approve(&self, id: &str, decision: ApprovalDecision) -> Result<()> {
        self.respond(events::approval_response(id, decision)?)
    }

    pub(crate) fn frontend(
        &self,
        program: &Path,
        socket: &Path,
        arguments: &[OsString],
    ) -> Result<std::process::Command> {
        Ok(self
            .current()?
            .native
            .frontend(program, socket, arguments)?)
    }
}
