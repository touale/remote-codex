mod actions;
pub(crate) mod approvals;
mod binding;
mod events;
pub(crate) mod gateway;
pub(crate) mod history;
mod lease;
pub(crate) mod permissions;
mod recovery;
mod route;

use crate::{
    ClientError, Result,
    remote::{Remote, bridge::Bridge},
    store::LocalStore,
};
use remote_codex_adapter::thread::{Codex, OpenThread, Thread};
use remote_codex_core::session::{SessionBinding, SessionEvent};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{broadcast, watch};

/// Owns the local Codex process, execution channel and exclusive session lease.
pub(crate) struct LocalRuntime {
    native: Thread,
    pub(crate) binding: SessionBinding,
    pub(crate) remote: Arc<Remote>,
    store: LocalStore,
    bridge: Bridge,
    lease: lease::Lease,
    has_history: AtomicBool,
    skills: crate::extensions::skills::SkillMap,
    approvals: Arc<approvals::Approvals>,
    permissions: Arc<permissions::Permissions>,
    events: broadcast::Sender<SessionEvent>,
    native_events: broadcast::Sender<Value>,
    pending_approvals: Mutex<HashMap<String, bool>>,
    closed: watch::Sender<Option<Option<String>>>,
    event_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    shutdown_once: tokio::sync::OnceCell<()>,
}

pub(crate) struct OpenOptions<'a> {
    pub(crate) directory: &'a Path,
    pub(crate) program: &'a Path,
    pub(crate) path: &'a str,
    pub(crate) existing: Option<SessionBinding>,
    pub(crate) takeover: bool,
    pub(crate) mcp: crate::extensions::mcp::McpPlan,
    pub(crate) progress: Option<Arc<dyn Fn(crate::progress::PrepareEvent) + Send + Sync>>,
}

impl LocalRuntime {
    pub(crate) async fn open(
        store: &LocalStore,
        remote: Arc<Remote>,
        options: OpenOptions<'_>,
    ) -> Result<Arc<Self>> {
        let OpenOptions {
            directory,
            program,
            path,
            existing,
            takeover,
            mcp,
            progress,
        } = options;
        use crate::progress::{PrepareEvent, PrepareStage};
        let progress = progress.unwrap_or_else(|| Arc::new(|_| {}));
        let home = codex_home()?;
        let snapshot = store.config_snapshot(&remote.server.id).await?;
        let mode = match snapshot
            .effective
            .get(&crate::config::ConfigKey::ExecutionMode)
        {
            Some(crate::config::ConfigValue::Text(mode)) => mode.clone(),
            _ => return Err(ClientError::RemoteResponse),
        };
        if let Some(binding) = &existing {
            binding::validate(binding, &remote, &home, &mode)?;
        }
        let lease_id = existing
            .as_ref()
            .map(|b| b.session.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let lease = lease::Lease::acquire(directory, &lease_id, takeover).await?;
        let workspace = remote
            .call(remote_codex_protocol::Request::Workspace { path: path.into() })
            .await?;
        let cwd = workspace["path"]
            .as_str()
            .ok_or(ClientError::RemoteResponse)?;
        progress(PrepareEvent::Stage(PrepareStage::ConnectExecution));
        let approvals = Arc::new(approvals::Approvals::default());
        let permissions = Arc::new(permissions::Permissions::default());
        let bridge = Bridge::start(
            remote.clone(),
            snapshot.revision.saved,
            mcp.commands,
            approvals.clone(),
            permissions.clone(),
        )
        .await?;
        progress(PrepareEvent::Stage(PrepareStage::StartLocalCodex));
        let codex = match Codex::start(program, &home).await {
            Ok(codex) => codex,
            Err(error) => {
                bridge.detach().await;
                return Err(error.into());
            }
        };
        let environment = format!("rc_{}", remote.server.id.replace('-', ""));
        let setup = async {
            codex
                .register_environment(&environment, &bridge.url)
                .await?;
            let skills =
                crate::extensions::skills::SkillMap::prepare(&codex, &remote, &home, &*progress)
                    .await?;
            progress(PrepareEvent::Stage(PrepareStage::OpenLocalSession));
            let opened = codex
                .open(OpenThread {
                    environment: &environment,
                    directory: cwd,
                    execution_mode: &mode,
                    existing: existing.as_ref().map(|b| b.session.id.as_str()),
                    mcp: mcp.config,
                    instructions: skills.instructions(),
                })
                .await?;
            let binding = SessionBinding {
                server_id: remote.server.id.clone(),
                remote_identity: remote.identity.identity.clone(),
                environment_id: environment,
                codex_home: home.to_string_lossy().into_owned(),
                codex_version: remote_codex_adapter::catalog::VERSION.into(),
                execution_mode: mode,
                revision: snapshot.revision.saved,
                session: opened.session.clone(),
            };
            let full_access = opened.full_access;
            let native = codex.bind(opened, binding.clone(), existing.is_some())?;
            permissions.restore(&bridge.channel, &binding.session.id, full_access)?;
            let lease = if existing.is_none() {
                lease::Lease::acquire(directory, &binding.session.id, false).await?
            } else {
                lease
            };
            if existing.is_some() {
                store.save_session(&binding).await?;
            }
            store.remember_workspace(&remote.server.id, cwd).await?;
            Ok::<_, ClientError>((native, binding, skills, lease))
        }
        .await;
        let (native, binding, skills, lease) = match setup {
            Ok(prepared) => prepared,
            Err(error) => {
                bridge.detach().await;
                codex.shutdown().await;
                return Err(error);
            }
        };
        let runtime = Arc::new(Self {
            native,
            binding,
            remote,
            store: store.clone(),
            bridge,
            lease,
            skills,
            approvals,
            permissions,
            has_history: AtomicBool::new(existing.is_some()),
            events: broadcast::channel(256).0,
            native_events: broadcast::channel(256).0,
            pending_approvals: Mutex::new(HashMap::new()),
            closed: watch::channel(None).0,
            event_task: Mutex::new(None),
            shutdown_once: tokio::sync::OnceCell::new(),
        });
        events::start(&runtime);
        Ok(runtime)
    }

    pub(crate) async fn shutdown(&self) {
        self.shutdown_once
            .get_or_init(|| self.finish_shutdown())
            .await;
    }

    pub(crate) fn persisted_session_id(&self) -> Option<&str> {
        self.has_history
            .load(Ordering::Acquire)
            .then_some(self.binding.session.id.as_str())
    }

    async fn finish_shutdown(&self) {
        self.approvals.clear();
        self.permissions.clear();
        // Detach before native cleanup can terminate background commands.
        self.bridge.detach().await;
        if self.has_history.load(Ordering::Acquire)
            && let Ok(session) = self.native.summary().await
        {
            let mut binding = self.binding.clone();
            binding.session = session;
            let _ = self.store.save_session(&binding).await;
        }
        self.native.shutdown().await;
        self.lease.release();
        self.closed.send_if_modified(|state| {
            if state.is_none() {
                *state = Some(None);
                true
            } else {
                false
            }
        });
    }
}

pub(crate) fn codex_home() -> Result<PathBuf> {
    let path = match std::env::var_os("CODEX_HOME") {
        Some(value) => PathBuf::from(value),
        None => {
            PathBuf::from(std::env::var_os("HOME").ok_or(ClientError::Argument("HOME is not set"))?)
                .join(".codex")
        }
    };
    if !path.is_absolute() {
        return Err(ClientError::Argument(
            "CODEX_HOME must be an absolute local directory",
        ));
    }
    std::fs::create_dir_all(&path)?;
    Ok(path.canonicalize()?)
}

impl Drop for LocalRuntime {
    fn drop(&mut self) {
        self.bridge.close();
        self.native.stop();
        if let Ok(mut task) = self.event_task.lock()
            && let Some(task) = task.take()
        {
            task.abort();
        }
    }
}
