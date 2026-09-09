mod actions;
pub(crate) mod approvals;
mod binding;
mod events;
pub(crate) mod gateway;
mod generation;
pub(crate) mod history;
mod intent;
mod lease;
pub(crate) mod permissions;
mod recovery;
mod route;
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
    has_history: AtomicBool,
    recipe: Recipe,
    pub(crate) recovery: Arc<Recovery>,
    intent: Mutex<intent::Intent>,
    turn_gate: tokio::sync::Mutex<()>,
    events: broadcast::Sender<SessionEvent>,
    native_events: broadcast::Sender<Value>,
    closed: watch::Sender<Option<Option<String>>>,
    event_task: Mutex<Vec<tokio::task::JoinHandle<()>>>,
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
        let home = codex_home()?;
        let snapshot = store.config_snapshot(&remote.server.id).await?;
        let mode = match snapshot
            .effective
            .get(&crate::config::ConfigKey::ExecutionMode)
        {
            Some(crate::config::ConfigValue::Text(mode)) => mode.clone(),
            _ => return Err(ClientError::RemoteResponse),
        };
        if let Some(binding) = &options.existing {
            binding::validate(binding, &remote, &home, &mode)?;
        }
        let lease_id = options
            .existing
            .as_ref()
            .map(|b| b.session.id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let lease = lease::Lease::acquire(options.directory, &lease_id, options.takeover).await?;
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
        let recipe = Recipe {
            program: options.program.into(),
            home,
            cwd,
            mode,
            revision: snapshot.revision.saved,
            mcp: options.mcp,
            config: snapshot.effective,
            project,
        };
        let recovery = Arc::new(Recovery::default());
        let progress = options.progress.unwrap_or_else(|| Arc::new(|_| {}));
        let (generation, binding) = recipe
            .open(
                remote.clone(),
                options.existing.as_ref(),
                recovery.clone(),
                &*progress,
                None,
            )
            .await?;
        let prepared = async {
            let lease = if options.existing.is_none() {
                lease::Lease::acquire(options.directory, &binding.session.id, false).await?
            } else {
                lease
            };
            if options.existing.is_some() {
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
        let runtime = Arc::new(Self {
            generation: RwLock::new(Arc::new(generation)),
            binding,
            remote,
            store: store.clone(),
            lease,
            recipe,
            recovery,
            has_history: AtomicBool::new(options.existing.is_some()),
            intent: Mutex::new(intent::Intent::default()),
            turn_gate: tokio::sync::Mutex::new(()),
            events: broadcast::channel(256).0,
            native_events: broadcast::channel(256).0,
            closed: watch::channel(None).0,
            event_task: Mutex::new(Vec::new()),
            shutdown_once: tokio::sync::OnceCell::new(),
        });
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
            if self.has_history.load(Ordering::Acquire)
                && let Ok(session) = generation.native.summary().await
            {
                let mut binding = self.binding.clone();
                binding.session = session;
                let _ = self.store.save_session(&binding).await;
            }
            generation.close().await;
        }
        self.lease.release();
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
