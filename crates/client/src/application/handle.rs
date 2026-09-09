use crate::{Result, local::LocalRuntime};
use remote_codex_core::session::{ApprovalDecision, Session, SessionEvent, SessionSettings};
use std::{ffi::OsString, path::PathBuf, sync::Arc};
use tokio::sync::{broadcast, watch};

/// A shared session owner. Independent processes coordinate through a lease.
#[derive(Clone)]
pub struct SessionHandle {
    runtime: Arc<LocalRuntime>,
    program: PathBuf,
}

pub struct TerminalAttachment {
    pub command: tokio::process::Command,
    pub closed: watch::Receiver<bool>,
    gateway: remote_codex_adapter::gateway::Gateway,
}

impl SessionHandle {
    pub(super) fn new(runtime: Arc<LocalRuntime>, program: PathBuf) -> Self {
        Self { runtime, program }
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
    pub async fn submit(&self, text: impl Into<String>) -> Result<String> {
        self.runtime.submit(text.into()).await
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
        let gateway = remote_codex_adapter::gateway::Gateway::start(self.runtime.clone()).await?;
        let command = self
            .runtime
            .frontend(&self.program, &gateway.socket, arguments)?;
        Ok(TerminalAttachment {
            command: command.into(),
            closed: gateway.closed.clone(),
            gateway,
        })
    }
}

impl TerminalAttachment {
    pub async fn finish(self) -> Result<()> {
        Ok(self.gateway.finish().await?)
    }
}
