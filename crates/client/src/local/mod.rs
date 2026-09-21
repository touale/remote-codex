mod actions;
pub(crate) mod approvals;
mod binding;
mod creation;
mod editing;
mod events;
pub(crate) mod gateway;
mod generation;
mod goals;
pub(crate) mod history;
mod intent;
pub(crate) mod lease;
mod message_recovery;
mod metadata;
pub(crate) mod permissions;
mod recovery;
mod route;
mod status;
mod supervisor;

use crate::{
    ClientError, Result,
    remote::{Remote, recovery::Recovery},
    store::LocalStore,
};
use generation::{Generation, Recipe};
use remote_codex_core::session::{EnvironmentState, SessionBinding, SessionEvent};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::{broadcast, watch};

/// Stable session owner. Replacing a generation never releases this lease or frontend.
pub(crate) struct LocalRuntime {
    generation: RwLock<Arc<Generation>>,
    pub(crate) binding: SessionBinding,
    pub(crate) remote: Arc<Remote>,
    store: LocalStore,
    lease: lease::Lease,
    workspace_lock: crate::workspace_lock::WorkspaceLock,
    has_history: AtomicBool,
    recipe: Recipe,
    pub(crate) recovery: Arc<Recovery>,
    intent: Mutex<intent::Intent>,
    turn_gate: tokio::sync::Mutex<()>,
    goal_gate: tokio::sync::Mutex<()>,
    events: broadcast::Sender<SessionEvent>,
    native_events: broadcast::Sender<Value>,
    closed: watch::Sender<Option<Option<String>>>,
    event_task: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    shutdown_once: tokio::sync::OnceCell<()>,
}

pub(crate) struct OpenOptions<'a> {
    pub(crate) directory: &'a Path,
    pub(crate) program: &'a remote_codex_adapter::program::Launch,
    pub(crate) path: &'a str,
    pub(crate) existing: Option<SessionBinding>,
    pub(crate) takeover: bool,
    pub(crate) mcp: crate::extensions::mcp::ProjectMcp,
    pub(crate) mcp_source: Option<String>,
    pub(crate) progress: Option<Arc<dyn Fn(crate::progress::PrepareEvent) + Send + Sync>>,
}

impl LocalRuntime {
    pub(crate) async fn open(
        store: &LocalStore,
        remote: Arc<Remote>,
        options: OpenOptions<'_>,
    ) -> Result<Arc<Self>> {
        let home = codex_home()?;
        let snapshot = store.config_snapshot(&remote.server.id).await?;
        let mode = execution_mode(&snapshot.effective)?.to_owned();
        if let Some(binding) = &options.existing {
            binding::validate(binding, &remote, &home, &mode)?;
        }
        // Reject invalid configuration before takeover can revoke another frontend.
        let mcp = options.mcp.resolve(
            &home,
            &format!("rc_{}", remote.server.id.replace('-', "")),
            options.mcp_source.as_deref(),
        )?;
        let lease = match &options.existing {
            Some(binding) => Some(
                lease::Lease::acquire(options.directory, &binding.session.id, options.takeover)
                    .await?,
            ),
            None => None,
        };
        let workspace = remote
            .call(remote_codex_protocol::Request::Workspace {
                path: options.path.into(),
            })
            .await?;
        let cwd = workspace["path"]
            .as_str()
            .ok_or(ClientError::RemoteResponse)?
            .to_owned();
        let project = remote
            .call(remote_codex_protocol::Request::ProjectConfig { path: cwd.clone() })
            .await?;
        let workspace_lock = crate::workspace_lock::WorkspaceLock::acquire(
            options.directory,
            &remote.server.id,
            Some(&cwd),
            false,
        )?;
        let recipe = Recipe {
            program: options.program.clone(),
            runtime_cache: options.directory.join("cache/runtime"),
            home,
            cwd,
            mode,
            revision: snapshot.revision.saved,
            mcp: options.mcp,
            mcp_source: options.mcp_source,
            config: snapshot.effective,
            project,
        };
        let recovery = Arc::new(Recovery::new(store.clone(), remote.server.id.clone()));
        let progress = options.progress.unwrap_or_else(|| Arc::new(|_| {}));
        let generation = recipe
            .open(
                remote.clone(),
                options
                    .existing
                    .as_ref()
                    .map_or(remote_codex_adapter::thread::OpenSource::New, |b| {
                        remote_codex_adapter::thread::OpenSource::Resume(&b.session.id)
                    }),
                mcp,
                recovery.clone(),
                &*progress,
                None,
            )
            .await?;
        Self::from_generation(
            store,
            remote,
            recipe,
            generation,
            recovery,
            (lease, workspace_lock),
            options.existing.is_some(),
        )
        .await
    }

    async fn from_generation(
        store: &LocalStore,
        remote: Arc<Remote>,
        recipe: Recipe,
        generation: Generation,
        recovery: Arc<Recovery>,
        locks: (Option<lease::Lease>, crate::workspace_lock::WorkspaceLock),
        persisted: bool,
    ) -> Result<Arc<Self>> {
        let (lease, workspace_lock) = locks;
        let binding = generation.native.binding().clone();
        let prepared = async {
            let lease = match lease {
                Some(lease) => lease,
                None => lease::Lease::acquire(&store.directory, &binding.session.id, false).await?,
            };
            if persisted {
                store.save_session(&binding).await?;
            }
            store
                .remember_workspace(&remote.server.id, &recipe.cwd)
                .await?;
            Ok::<_, ClientError>(lease)
        }
        .await;
        let lease = match prepared {
            Ok(lease) => lease,
            Err(error) => {
                generation.close().await;
                return Err(error);
            }
        };
        let status = generation.native.initial_status();
        let runtime = Arc::new(Self {
            generation: RwLock::new(Arc::new(generation)),
            binding,
            remote,
            store: store.clone(),
            lease,
            recipe,
            workspace_lock,
            recovery,
            has_history: AtomicBool::new(persisted),
            intent: Mutex::new(intent::Intent::with_status(status)),
            turn_gate: tokio::sync::Mutex::new(()),
            goal_gate: tokio::sync::Mutex::new(()),
            events: broadcast::channel(256).0,
            native_events: broadcast::channel(256).0,
            closed: watch::channel(None).0,
            event_task: Mutex::new(Vec::new()),
            shutdown_once: tokio::sync::OnceCell::new(),
        });
        if let Err(error) = runtime.load_goal().await {
            runtime.shutdown().await;
            return Err(error);
        }
        events::start(&runtime)?;
        Ok(runtime)
    }

    fn current(&self) -> Result<Arc<Generation>> {
        self.generation
            .read()
            .map(|g| g.clone())
            .map_err(|_| ClientError::RemoteResponse)
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
        self.closed.send_if_modified(|state| {
            if state.is_none() {
                *state = Some(None);
                true
            } else {
                false
            }
        });
        self.recovery.publish(EnvironmentState::Closed);
        if let Ok(generation) = self.current() {
            generation.detach().await;
            let _ = self.refresh_summary(&generation).await;
            generation.close().await;
        }
        self.lease.release();
        self.workspace_lock.release();
    }
}

pub(crate) fn execution_mode(config: &crate::config::EffectiveConfig) -> Result<&str> {
    match config.get(&crate::config::ConfigKey::ExecutionMode) {
        Some(crate::config::ConfigValue::Text(mode)) => Ok(mode),
        _ => Err(ClientError::RemoteResponse),
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
        if let Ok(generation) = self.generation.get_mut() {
            generation.bridge.close();
            generation.native.stop();
        }
        if let Ok(mut tasks) = self.event_task.lock() {
            for task in tasks.drain(..) {
                task.abort();
            }
        }
    }
}
