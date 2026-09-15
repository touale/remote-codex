use crate::{Result, local::LocalRuntime};
use remote_codex_core::session::{ApprovalDecision, Session, SessionEvent, SessionSettings};
use std::{ffi::OsString, path::PathBuf, sync::Arc};
use tokio::sync::{broadcast, watch};

/// A shared session owner. Independent processes coordinate through a lease.
#[derive(Clone)]
pub struct SessionHandle {
    pub(super) runtime: Arc<LocalRuntime>,
    pub(super) program: PathBuf,
    pub(super) client: super::Client,
}

pub struct TerminalAttachment {
    pub command: tokio::process::Command,
    pub closed: watch::Receiver<bool>,
    pub current: watch::Receiver<SessionHandle>,
    gateway: remote_codex_adapter::gateway::Gateway,
}

impl SessionHandle {
    pub fn snapshot(&self) -> Result<super::SessionSnapshot> {
        self.runtime.snapshot()
    }
    pub fn interact(&self, id: &str, answer: super::InteractionAnswer) -> Result<()> {
        self.runtime.interact(id, answer)
    }
    pub fn current_settings(&self) -> Result<super::ConfirmedSettings> {
        self.runtime.current_settings()
    }
    pub async fn models(&self) -> Result<Vec<super::ModelOption>> {
        self.runtime.models().await
    }
    pub async fn mcp_status(&self) -> Result<Vec<super::McpStatus>> {
        self.runtime.mcp_status().await
    }
    pub(super) fn new(runtime: Arc<LocalRuntime>, program: PathBuf, client: super::Client) -> Self {
        Self {
            runtime,
            program,
            client,
        }
    }
    pub fn session(&self) -> &Session {
        &self.runtime.binding.session
    }
    pub fn server(&self) -> &str {
        &self.runtime.remote.server.name
    }
    pub fn persisted_id(&self) -> Option<&str> {
        self.runtime.persisted_session_id()
    }
    pub fn events(&self) -> broadcast::Receiver<SessionEvent> {
        self.runtime.events()
    }
    pub fn environment(&self) -> watch::Receiver<remote_codex_core::session::EnvironmentState> {
        self.runtime.recovery.state.subscribe()
    }
    pub fn retry(&self) {
        self.runtime.retry();
    }
    /// Invoke only while the frontend owns a safe interactive authentication surface.
    pub async fn authenticate(&self) -> Result<()> {
        self.runtime.authenticate().await
    }
    pub async fn message(
        &self,
        text: String,
        client_id: String,
        expected_turn: Option<String>,
    ) -> Result<super::Submission> {
        self.runtime.message(text, client_id, expected_turn).await
    }

    pub async fn revert(&self, before_turn: &str) -> Result<super::RevertedSession> {
        self.runtime.revert(before_turn).await
    }

    pub async fn status(&self) -> Result<super::SessionSnapshot> {
        self.runtime.status().await
    }
    pub async fn goal(&self, action: super::GoalAction) -> Result<Option<super::Goal>> {
        self.runtime.goal(action).await
    }
    pub async fn settings(&self, settings: SessionSettings) -> Result<()> {
        self.runtime.settings(settings).await
    }
    pub fn approve(&self, request_id: &str, decision: ApprovalDecision) -> Result<()> {
        self.runtime.approve(request_id, decision)
    }
    pub async fn interrupt(&self, turn_id: &str) -> Result<()> {
        self.runtime.interrupt(turn_id).await
    }
    pub async fn close(&self) {
        self.runtime.shutdown().await;
    }

    pub async fn terminal(&self, arguments: &[OsString]) -> Result<TerminalAttachment> {
        let backend = super::terminal::TerminalSessions::new(self.clone());
        let current = backend.current.subscribe();
        let gateway = remote_codex_adapter::gateway::Gateway::start(backend).await?;
        let command = self
            .runtime
            .frontend(&self.program, &gateway.socket, arguments)?;
        Ok(TerminalAttachment {
            command: command.into(),
            closed: gateway.closed.clone(),
            current,
            gateway,
        })
    }
}

impl TerminalAttachment {
    pub async fn finish(self) -> Result<()> {
        Ok(self.gateway.finish().await?)
    }
}
