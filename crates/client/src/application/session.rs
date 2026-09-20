use super::{CachedSession, Client, HistoryPage, SessionHandle};
use crate::{
    ClientError, Result,
    extensions::mcp::ProjectMcp,
    local::{LocalRuntime, OpenOptions},
    remote::Remote,
};
use remote_codex_core::session::SessionBinding;
use std::sync::Arc;

pub struct SessionService {
    pub(super) client: Client,
}

#[derive(Clone, Debug)]
pub struct OpenSession {
    pub server: String,
    pub path: String,
    pub resume: Option<String>,
    pub takeover: bool,
    pub mcp_source: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProjectTrust {
    pub server: String,
    pub path: String,
    pub names: Vec<String>,
    pub required: bool,
}

/// Holds a connected environment and an inspected configuration snapshot.
/// Trust applies only to this snapshot's digest, never to later file changes.
pub struct PreparedSession {
    client: Client,
    options: OpenSession,
    remote: Arc<Remote>,
    project: ProjectMcp,
    program: remote_codex_adapter::program::Launch,
    existing: Option<SessionBinding>,
}

impl SessionService {
    pub async fn list(&self, server: Option<&str>, archived: bool) -> Result<Vec<CachedSession>> {
        let state = &self.client.0;
        let id = match server {
            Some(name) => Some(state.store.find_connection(name).await?.id),
            None => None,
        };
        state.store.cached_sessions(id.as_deref(), archived).await
    }

    pub async fn resolve(&self, id: &str, server: Option<&str>) -> Result<CachedSession> {
        crate::sessions::resolve(&self.client.0.store, id, server).await
    }

    pub async fn read(&self, id: &str, cursor: Option<&str>) -> Result<HistoryPage> {
        let program = self.client.0.program().await?;
        let binding = self.client.0.store.session_binding(id).await?;
        let mut page = crate::local::history::read(&program, &binding, cursor).await?;
        self.client.0.store.message_times(&mut page).await?;
        self.client.0.store.save_summary(&page.session).await?;
        Ok(page)
    }

    pub async fn prepare(&self, options: OpenSession) -> Result<PreparedSession> {
        self.client.0.ensure_open()?;
        let program = self.client.0.program().await?;
        let state = &self.client.0;
        let record = state.store.find_connection(&options.server).await?;
        let existing = match &options.resume {
            Some(id) => {
                let binding = state.store.session_binding(id).await?;
                if binding.server_id != record.id || binding.session.cwd != options.path {
                    return Err(ClientError::Argument(
                        "resume must preserve the bound server and directory",
                    ));
                }
                Some(binding)
            }
            None => None,
        };
        if !options.path.starts_with('/') || options.path.chars().any(char::is_control) {
            return Err(ClientError::Argument(
                "workspace must be a remote absolute path",
            ));
        }
        let remote = state.connect(record).await?;
        let project = crate::extensions::mcp::inspect(&state.store, &remote, &options.path).await?;
        Ok(PreparedSession {
            client: self.client.clone(),
            options,
            remote,
            project,
            program,
            existing,
        })
    }
}

impl PreparedSession {
    pub fn trust(&self) -> ProjectTrust {
        ProjectTrust {
            server: self.options.server.clone(),
            path: self.project.path.clone(),
            names: self.project.names.clone(),
            required: !self.project.names.is_empty() && !self.project.trusted,
        }
    }

    pub async fn open(mut self, trust_project: bool) -> Result<SessionHandle> {
        let state = &self.client.0;
        state.ensure_open()?;
        if trust_project && !self.project.trusted {
            self.project
                .trust(&state.store, &self.remote.server.id)
                .await?;
        }
        let mcp = self.project.resolve(
            &crate::local::codex_home()?,
            &format!("rc_{}", self.remote.server.id.replace('-', "")),
            self.options.mcp_source.as_deref(),
        )?;
        let runtime = LocalRuntime::open(
            &state.store,
            self.remote,
            OpenOptions {
                directory: &state.directory,
                program: &self.program,
                path: &self.options.path,
                existing: self.existing,
                takeover: self.options.takeover,
                mcp,
                progress: crate::progress::current().or_else(|| state.options.progress.clone()),
            },
        )
        .await?;
        if let Err(error) = state.register(&runtime) {
            runtime.shutdown().await;
            return Err(error);
        }
        Ok(SessionHandle::new(runtime, self.program, self.client))
    }
}

impl SessionService {
    pub async fn update_metadata(
        &self,
        id: &str,
        name: Option<String>,
        archived: Option<bool>,
    ) -> Result<()> {
        if name.as_ref().is_some_and(|n| {
            n.trim().is_empty() || n.len() > 256 || n.chars().any(char::is_control)
        }) {
            return Err(ClientError::Argument(
                "Session name must contain 1 to 256 characters.",
            ));
        }
        let state = &self.client.0;
        let mut binding = state.store.session_binding(id).await?;
        let _workspace = crate::workspace_lock::WorkspaceLock::acquire(
            &state.directory,
            &binding.server_id,
            Some(&binding.session.cwd),
            false,
        )?;
        let _lease = crate::local::lease::Lease::acquire(&state.directory, id, false).await?;
        let program = state.program().await?;
        remote_codex_adapter::thread::history::update(
            &program,
            &binding,
            name.as_deref(),
            archived,
        )
        .await?;
        if let Some(name) = name {
            binding.session.title = name;
        }
        if let Some(archived) = archived {
            binding.session.archived = archived;
        }
        state.store.save_session(&binding).await
    }
}
