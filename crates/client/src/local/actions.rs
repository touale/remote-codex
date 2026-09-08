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
        let prepared = self.native.prepare_action(
            action,
            self.permissions.full_access(),
            self.has_history.load(Ordering::Acquire),
            self.skills.mappings(),
        )?;
        Ok(route::execute(self, prepared).await?)
    }

    pub(crate) fn events(&self) -> tokio::sync::broadcast::Receiver<SessionEvent> {
        self.events.subscribe()
    }

    pub(crate) async fn submit(&self, text: String) -> Result<String> {
        Ok(turn_id(&self.action(Action::Submit(text)).await?)?)
    }

    pub(crate) async fn settings(&self, settings: SessionSettings) -> Result<()> {
        self.action(Action::Settings(settings)).await?;
        Ok(())
    }

    pub(crate) async fn interrupt(&self, turn: &str) -> Result<()> {
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
        Ok(self.native.frontend(program, socket, arguments)?)
    }
}
