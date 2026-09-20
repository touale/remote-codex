use super::*;
use crate::{
    extensions::{mcp::McpPlan, skills::SkillMap},
    progress::{PrepareEvent, PrepareStage},
    remote::bridge::Bridge,
};
use remote_codex_adapter::thread::{Codex, OpenSource, OpenThread, Thread};
use std::collections::HashMap;

#[derive(Clone)]
pub(super) struct Recipe {
    pub program: remote_codex_adapter::program::Launch,
    pub runtime_cache: PathBuf,
    pub home: PathBuf,
    pub cwd: String,
    pub mode: String,
    pub revision: i64,
    pub mcp: McpPlan,
    pub project: Value,
    pub config: crate::config::EffectiveConfig,
}

pub(super) struct PendingRequest {
    pub event: Value,
    pub command_approval: bool,
}

pub(super) struct Generation {
    pub native: Thread,
    pub bridge: Bridge,
    pub skills: SkillMap,
    pub approvals: Arc<approvals::Approvals>,
    pub permissions: Arc<permissions::Permissions>,
    pub pending_requests: Mutex<HashMap<String, PendingRequest>>,
    detached: tokio::sync::OnceCell<()>,
    closed: tokio::sync::OnceCell<()>,
}

impl Recipe {
    pub(super) async fn open(
        &self,
        remote: Arc<Remote>,
        source: OpenSource<'_>,
        recovery: Arc<Recovery>,
        progress: &(dyn Fn(PrepareEvent) + Send + Sync),
        expected_skills: Option<&SkillMap>,
    ) -> Result<Generation> {
        let program = remote_codex_adapter::program::inspect(&self.program).await?;
        self.mcp.verify_local(&self.home)?;
        let runtime = crate::runtime::prepare(
            &remote.ssh,
            &remote.server.endpoint,
            &self.runtime_cache,
            &program.version,
            remote.server.runtime.as_ref(),
            progress,
        )
        .await?;
        program.verify()?;
        progress(PrepareEvent::Stage(PrepareStage::ConnectExecution));
        let approvals = Arc::new(approvals::Approvals::default());
        let permissions = Arc::new(permissions::Permissions::default());
        let bridge = Bridge::start(
            remote.clone(),
            self.revision,
            runtime.reference(),
            self.mcp.commands.clone(),
            approvals.clone(),
            permissions.clone(),
            recovery,
        )
        .await?;
        progress(PrepareEvent::Stage(PrepareStage::StartLocalCodex));
        let mut launch = self.program.clone();
        launch.path.clone_from(&program.path);
        let codex = match Codex::start(&launch, &self.home).await {
            Ok(codex) => codex,
            Err(error) => {
                bridge.detach().await;
                return Err(error.into());
            }
        };
        let environment = format!("rc_{}", remote.server.id.replace('-', ""));
        let prepared = async {
            program.verify()?;
            codex
                .register_environment(&environment, &bridge.url)
                .await?;
            let skills =
                SkillMap::prepare(&codex, &remote, &self.home, progress, expected_skills).await?;
            progress(PrepareEvent::Stage(PrepareStage::OpenLocalSession));
            let opened = codex
                .open(OpenThread {
                    environment: &environment,
                    directory: &self.cwd,
                    execution_mode: &self.mode,
                    source,
                    mcp: self.mcp.config.clone(),
                    instructions: skills.instructions(),
                })
                .await?;
            let full = opened.full_access;
            let binding = SessionBinding {
                server_id: remote.server.id.clone(),
                remote_identity: remote.identity.identity.clone(),
                environment_id: environment,
                codex_home: self.home.to_string_lossy().into_owned(),
                codex_version: program.version.clone(),
                execution_mode: self.mode.clone(),
                revision: self.revision,
                session: opened.session.clone(),
            };
            if matches!(source, OpenSource::Resume(id) if id != binding.session.id) {
                return Err(ClientError::RemoteResponse);
            }
            let restoring = match source {
                OpenSource::Resume(_) => true,
                OpenSource::Frontend(creation) => creation.allow_full_access,
                OpenSource::New => false,
            };
            let id = binding.session.id.clone();
            let native = codex.bind(opened, binding, restoring).map_err(|error| {
                if matches!(source, OpenSource::Frontend(_)) {
                    remote_codex_protocol::Fault::unknown(&format!("Codex created thread {id}, but its execution settings could not be validated: {error}. Do not repeat creation automatically."))
                } else { error }
            })?;
            if matches!(source, OpenSource::Resume(_)) && expected_skills.is_some() {
                native.pause_goal_for_recovery().await?;
            }
            permissions.restore(&bridge.channel, &native.binding().session.id, full)?;
            Ok::<_, ClientError>((native, skills))
        }
        .await;
        match prepared {
            Ok((native, skills)) => Ok(Generation {
                native,
                bridge,
                skills,
                approvals,
                permissions,
                pending_requests: Mutex::new(HashMap::new()),
                detached: tokio::sync::OnceCell::new(),
                closed: tokio::sync::OnceCell::new(),
            }),
            Err(error) => {
                bridge.detach().await;
                codex.shutdown().await;
                Err(error)
            }
        }
    }
}

impl Generation {
    pub(super) async fn detach(&self) {
        self.detached.get_or_init(|| self.bridge.detach()).await;
    }

    pub(super) async fn close(&self) {
        self.closed
            .get_or_init(|| async {
                self.approvals.clear();
                self.permissions.clear();
                self.detach().await;
                self.native.shutdown().await;
            })
            .await;
    }
}

impl Drop for Generation {
    fn drop(&mut self) {
        self.bridge.close();
        self.native.stop();
    }
}
