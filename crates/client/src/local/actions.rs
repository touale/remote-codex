use super::{LocalRuntime, route};
use crate::Result;
use remote_codex_adapter::{
    events,
    thread::action::{Action, turn_id},
};
use remote_codex_core::session::{ApprovalDecision, SessionEvent, SessionSettings};
use std::{ffi::OsString, path::Path, sync::atomic::Ordering};

impl LocalRuntime {
    async fn action(&self, action: Action) -> Result<serde_json::Value> {
        let generation = self.current()?;
        if let Action::Interrupt(turn) = &action {
            self.cancel_continuation(turn)?;
        }
        let prepared = generation.native.prepare_action(
            action,
            generation.permissions.full_access(),
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
            generation.permissions.full_access(),
            self.has_history.load(Ordering::Acquire),
            generation.skills.mappings(),
        )?;
        Ok(turn_id(
            &route::execute_recovery(self, &generation, prepared, interrupted_turn).await?,
        )?)
    }

    pub(crate) async fn submit(&self, text: String) -> Result<String> {
        Ok(turn_id(&self.action(Action::Submit(text)).await?)?)
    }

    pub(crate) async fn settings(&self, settings: SessionSettings) -> Result<()> {
        self.action(Action::Settings(settings)).await?;
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
        if self.recovery.ready().is_err() {
            self.cancel_continuation(turn)?;
            let _ = self.current()?.native.queue_interrupt(turn);
            return Ok(());
        }
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
