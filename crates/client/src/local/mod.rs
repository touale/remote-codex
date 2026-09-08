pub(crate) mod approvals;
mod binding;
mod execution_policy;
pub mod gateway;
pub mod history;
mod lease;
pub(crate) mod permissions;
mod recovery;
mod route;
mod settings;

use crate::{
    ClientError, Result,
    remote::{Remote, bridge::Bridge},
    store::LocalStore,
};
use remote_codex_adapter::engine::Engine;
use remote_codex_protocol::SessionBinding;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// A frontend-owned local runtime. App tabs share this handle; independent CLI
/// processes have isolated engines and cooperate through a per-thread lease.
pub struct LocalRuntime {
    pub engine: Engine,
    pub initialized: Value,
    pub binding: SessionBinding,
    pub remote: Arc<Remote>,
    pub(crate) store: LocalStore,
    pub(crate) bridge: Bridge,
    pub(crate) lease: lease::Lease,
    pub(crate) bootstrap: Value,
    pub(crate) has_history: AtomicBool,
    pub(crate) skills: crate::extensions::skills::SkillMap,
    pub(crate) approvals: Arc<approvals::Approvals>,
    pub(crate) permissions: Arc<permissions::Permissions>,
    shutdown_once: tokio::sync::OnceCell<()>,
}

pub struct OpenOptions<'a> {
    pub directory: &'a Path,
    pub program: &'a Path,
    pub path: &'a str,
    pub existing: Option<SessionBinding>,
    pub takeover: bool,
    pub mcp: crate::extensions::mcp::McpPlan,
    pub progress: Option<Arc<dyn Fn(crate::progress::PrepareEvent) + Send + Sync>>,
}

impl LocalRuntime {
    pub async fn open(
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
        let (engine, initialized) = Engine::local(program, &home).await?;
        let environment_id = format!("rc_{}", remote.server.id.replace('-', ""));
        let setup = async {
            engine.call("environment/add", json!({"environmentId":environment_id,"execServerUrl":bridge.url,"connectTimeoutMs":15000})).await?;
            let skills = crate::extensions::skills::SkillMap::prepare(&engine, &remote, &home, &*progress).await?;
            let mut params = binding::start_params(&environment_id, cwd, &mode);
            for (key, value) in mcp.config { params["config"][key] = value; }
            let resources=skills.instructions();
            if !resources.is_empty() {
                let native=engine.call("config/read",json!({"includeLayers":false})).await?;
                let inherited=native["config"]["developer_instructions"].as_str().unwrap_or_default();
                params["developerInstructions"]=json!(format!("{inherited}\n\n{resources}"));
            }
            params["dynamicTools"] = recovery::tools();
            progress(PrepareEvent::Stage(PrepareStage::OpenLocalSession));
            let response = if let Some(binding) = &existing {
                let mut params = params;
                let object = params.as_object_mut().ok_or(ClientError::RemoteResponse)?;
                for key in ["environments", "sandbox", "approvalPolicy"] { object.remove(key); }
                params["threadId"] = json!(binding.session.id);
                engine.call("thread/resume", params).await?
            } else { engine.call("thread/start", params).await? };
            let session = binding::session(&response["thread"], cwd)?;
            let binding = SessionBinding {server_id: remote.server.id.clone(), remote_identity: remote.identity.identity.clone(),
                environment_id, codex_home: home.to_string_lossy().into_owned(), codex_version: crate::runtime::CANDIDATE_VERSION.into(),
                execution_mode: mode, revision: snapshot.revision.saved, session};
            execution_policy::validate(&json!({"cwd": response["cwd"], "sandboxPolicy": response["sandbox"], "approvalsReviewer": response["approvalsReviewer"]}), &binding, existing.is_some())?;
            permissions.restore(&bridge.channel, &binding.session.id, &response["sandbox"])?;
            if existing.is_some() {store.save_session(&binding).await?;}
            store.remember_workspace(&remote.server.id, cwd).await?;
            Ok::<_, ClientError>((binding, response, skills))
        }.await;
        match setup {
            Ok((binding, bootstrap, skills)) => {
                let lease = if existing.is_none() {
                    lease::Lease::acquire(directory, &binding.session.id, false).await?
                } else {
                    lease
                };
                Ok(Arc::new(Self {
                    engine,
                    initialized,
                    binding,
                    remote,
                    store: store.clone(),
                    bridge,
                    lease,
                    bootstrap,
                    skills,
                    approvals,
                    permissions,
                    has_history: AtomicBool::new(existing.is_some()),
                    shutdown_once: tokio::sync::OnceCell::new(),
                }))
            }
            Err(error) => {
                bridge.detach().await;
                engine.shutdown().await;
                Err(error)
            }
        }
    }

    pub async fn shutdown(&self) {
        self.shutdown_once
            .get_or_init(|| self.finish_shutdown())
            .await;
    }

    /// A catalogued session can be named in recovery guidance; empty drafts cannot.
    pub fn persisted_session_id(&self) -> Option<&str> {
        self.has_history
            .load(Ordering::Acquire)
            .then_some(self.binding.session.id.as_str())
    }

    async fn finish_shutdown(&self) {
        self.approvals.clear();
        self.permissions.clear();
        // Close execution subscriptions before Codex's exit cleanup can terminate
        // commands whose server explicitly permits background execution.
        self.bridge.detach().await;
        if self.has_history.load(Ordering::Acquire)
            && let Ok(response) = self
                .engine
                .call(
                    "thread/read",
                    json!({"threadId":self.binding.session.id,"includeTurns":false}),
                )
                .await
            && let Ok(session) = binding::session(&response["thread"], &self.binding.session.cwd)
        {
            let mut binding = self.binding.clone();
            binding.session = session;
            let _ = self.store.save_session(&binding).await;
        }
        self.engine.shutdown().await;
    }
}

pub fn codex_home() -> Result<PathBuf> {
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
        self.engine.stop();
    }
}
