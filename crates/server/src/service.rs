use crate::{
    Checked, Result,
    config::{Runtime, canonical_directory},
    execution::{Execution, Input},
    storage::Store,
};
use remote_codex_protocol::{Call, Fault, Hello, Request, VERSION};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
};
use tokio::sync::Mutex;

pub struct Service {
    pub store: Store,
    root: PathBuf,
    executions: Mutex<HashMap<String, Arc<Execution>>>,
    build_id: String,
}
impl Service {
    pub async fn open(root: &Path) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            store: Store::open(root).await?,
            root: root.into(),
            executions: Mutex::new(HashMap::new()),
            build_id: crate::installation::build_id()?,
        }))
    }
    pub(crate) fn validate(&self, call: &Call) -> Result<()> {
        if call.protocol != VERSION {
            return Err(Fault::new(
                "PROTOCOL_MISMATCH",
                "client and execution service versions differ",
            ));
        }
        if call
            .expected_identity
            .as_ref()
            .is_some_and(|id| id != &self.store.identity)
        {
            return Err(Fault::new(
                "REMOTE_IDENTITY_CHANGED",
                "execution environment identity changed",
            ));
        }
        if call.profile.is_empty()
            || call.profile.len() > 256
            || call.profile.chars().any(char::is_control)
        {
            return Err(Fault::new("INVALID_PROFILE", "invalid environment owner"));
        }
        Ok(())
    }
    pub async fn dispatch(self: &Arc<Self>, call: &Call) -> Result<Value> {
        self.validate(call)?;
        match &call.request {
            Request::Hello => encode(Hello {
                protocol: VERSION,
                identity: self.store.identity.clone(),
                home: std::env::var("HOME").unwrap_or_default(),
                user: std::env::var("USER").unwrap_or_default(),
                version: env!("CARGO_PKG_VERSION").into(),
                capabilities: vec![
                    remote_codex_protocol::COMMAND_APPROVAL_CAPABILITY.into(),
                    remote_codex_protocol::SESSION_PERMISSIONS_CAPABILITY.into(),
                    remote_codex_protocol::IDLE_RETIREMENT_CAPABILITY.into(),
                ],
                build_id: self.build_id.clone(),
            }),
            Request::PrepareUpdate => {
                for execution in self.executions.lock().await.values() {
                    let _ = execution.send(Input::RetireIdle);
                }
                Ok(json!({}))
            }
            Request::Configure(config) => {
                Runtime::load(config.clone()).await?;
                self.store.save_config(&call.profile, config).await?;
                Ok(json!({"revision":config.revision}))
            }
            Request::Workspace { path } => Ok(json!({"path":canonical_directory(path).await?})),
            Request::ProjectConfig { path } => {
                let workspace = canonical_directory(path).await?;
                let config = Path::new(&workspace).join(".codex/config.toml");
                if !config.exists() {
                    return Ok(json!({"path":workspace,"config":null}));
                }
                let actual = tokio::fs::canonicalize(&config)
                    .await
                    .checked("PROJECT_CONFIG", "cannot inspect project config")?;
                if !actual.starts_with(&workspace)
                    || actual
                        .metadata()
                        .checked("PROJECT_CONFIG", "cannot inspect project config")?
                        .len()
                        > 256 * 1024
                {
                    return Err(Fault::new(
                        "PROJECT_CONFIG",
                        "project config escapes workspace or exceeds 256 KiB",
                    ));
                }
                let text = tokio::fs::read_to_string(actual)
                    .await
                    .checked("PROJECT_CONFIG", "cannot read project MCP configuration")?;
                Ok(json!({"path":workspace,"config":text}))
            }
            Request::PrepareSkill { digest, files } => {
                crate::skills::prepare(&self.root, digest, files).await
            }
            Request::CommitSkill { stage, digest } => {
                crate::skills::commit(&self.root, stage, digest).await
            }
            Request::OpenExecution {
                channel,
                revision,
                mcp,
            } => {
                uuid::Uuid::parse_str(channel)
                    .map_err(|_| Fault::new("INVALID_EXECUTION", "invalid execution identity"))?;
                let mut executions = self.executions.lock().await;
                executions.retain(|_, v| v.alive.load(Ordering::Acquire));
                if let Some(owner) = self.store.channel_owner(channel).await? {
                    if owner != call.profile {
                        return Err(Fault::new(
                            "EXECUTION_DENIED",
                            "execution belongs to another owner",
                        ));
                    }
                    if executions.contains_key(channel) {
                        return Ok(json!({"channel":channel}));
                    }
                    return Err(Fault::unknown(
                        "previous execution backend stopped; create a new execution channel",
                    ));
                }
                if executions.len() >= 64 {
                    return Err(Fault::new(
                        "CAPACITY_EXCEEDED",
                        "too many active execution channels",
                    ));
                }
                let config = self.store.config(&call.profile).await?;
                if config.revision != *revision {
                    return Err(Fault::new(
                        "REVISION_CONFLICT",
                        "environment configuration changed before execution",
                    ));
                }
                let mut runtime = Runtime::load(config).await?;
                if mcp.len() > 32
                    || mcp
                        .iter()
                        .any(|cmd| cmd.argv.is_empty() || !Path::new(&cmd.cwd).is_absolute())
                {
                    return Err(Fault::new("INVALID_MCP", "invalid authorized MCP command"));
                }
                runtime.mcp = mcp.clone();
                let execution = Execution::start(
                    &self.root,
                    channel,
                    &call.profile,
                    runtime,
                    self.store.clone(),
                )
                .await?;
                executions.insert(channel.clone(), execution);
                Ok(json!({"channel":channel}))
            }
            Request::DetachExecution { channel } => {
                self.execution(&call.profile, channel)
                    .await?
                    .send(Input::Detached)?;
                Ok(json!({}))
            }
            Request::Jobs { thread } => {
                encode(self.store.jobs(&call.profile, thread.as_deref()).await?)
            }
            Request::JobOutput { id, after } => {
                self.store.job_output(&call.profile, id, *after).await
            }
            Request::AttachExecution { .. } => Err(Fault::new(
                "INVALID_REQUEST",
                "execution attach requires a streaming connection",
            )),
        }
    }
    pub(crate) async fn execution(&self, profile: &str, channel: &str) -> Result<Arc<Execution>> {
        if self.store.channel_owner(channel).await?.as_deref() != Some(profile) {
            return Err(Fault::new(
                "EXECUTION_DENIED",
                "execution does not belong to this environment",
            ));
        }
        self.executions
            .lock()
            .await
            .get(channel)
            .cloned()
            .ok_or_else(|| Fault::unknown("execution backend is no longer available"))
    }
    pub async fn shutdown(&self) {
        for execution in self.executions.lock().await.values() {
            let _ = execution.send(Input::Stop);
        }
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(4);
        while tokio::time::Instant::now() < deadline {
            if self
                .executions
                .lock()
                .await
                .values()
                .all(|e| !e.alive.load(Ordering::Acquire))
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
    pub async fn serve(self: Arc<Self>, listener: tokio::net::UnixListener) -> Result<()> {
        let permits = Arc::new(tokio::sync::Semaphore::new(64));
        loop {
            let (stream, _) = listener
                .accept()
                .await
                .checked("CONNECTION_FAILED", "cannot accept execution client")?;
            let Ok(permit) = permits.clone().try_acquire_owned() else {
                continue;
            };
            let service = self.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let _ = crate::frontend::connection(service, stream).await;
            });
        }
    }
}
fn encode(value: impl serde::Serialize) -> Result<Value> {
    serde_json::to_value(value).checked("PROTOCOL_ERROR", "cannot encode execution result")
}
