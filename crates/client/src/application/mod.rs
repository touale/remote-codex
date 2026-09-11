//! Application boundary shared by the CLI and future desktop clients.
mod authentication;
mod config;
mod directory;
mod files;
mod handle;
mod native;
mod notice;
mod project_mcp;
mod server;
mod session;
mod shell;
mod transfer_creation;
mod transfers;
mod workspace;
pub use crate::ssh::interaction::{
    AuthenticationHandler, AuthenticationKind, AuthenticationPrompt,
};
pub use authentication::ServerAuthentication;
pub use directory::DirectoryBrowser;
pub use files::FileHandle;
pub use native::{LoginStart, NativeHandle, NativeService, NativeStatus};
pub use project_mcp::{ProjectMcpConfig, ProjectMcpServer};
pub use remote_codex_core::desktop::*;
pub use remote_codex_core::goals::*;
pub use remote_codex_core::status::{AccountUsage, Submission};
pub use remote_codex_protocol::{DirectoryPage, TextFile};
pub use shell::{ShellEvent, ShellHandle};
pub use transfers::{
    Choice as TransferChoice, Conflict as TransferConflict, Direction as TransferDirection,
    Progress as TransferProgress, SkippedTransfers, Status as TransferStatus, Transfer,
    TransferService,
};
pub use workspace::{Workspace, WorkspaceHandle, WorkspaceService};

pub use crate::credentials::validate_password as validate_ssh_password;
pub use crate::servers::status::ServerStatus;
pub use config::ConfigService;
pub use handle::{SessionHandle, TerminalAttachment};
pub use notice::{ClientNotice, NoticeHandler};
pub use remote_codex_core::session::{
    ApprovalDecision, CachedSession, HistoryPage, Session, SessionEvent, SessionSettings,
};
pub use server::{AddServer, ServerList, ServerService, ServerSummary};
pub use session::{OpenSession, PreparedSession, ProjectTrust, SessionService};

use crate::{Result, progress::PrepareEvent, remote::Remote, store::LocalStore};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, Weak},
};

#[derive(Clone, Default)]
pub struct ServiceBundle {
    pub bytes: Arc<[u8]>,
    pub sha256: String,
}

pub type Progress = Arc<dyn Fn(PrepareEvent) + Send + Sync>;

#[derive(Clone, Default)]
pub struct ClientOptions {
    pub codex_program: Option<PathBuf>,
    pub desktop_discovery: bool,
    pub authentication: Option<AuthenticationHandler>,
    pub data_dir: Option<PathBuf>,
    pub ssh_config: Option<PathBuf>,
    pub interactive: bool,
    pub service: ServiceBundle,
    pub progress: Option<Progress>,
    pub notice: Option<NoticeHandler>,
}

#[derive(Clone)]
pub struct Client(Arc<State>);

struct State {
    selected_program: Mutex<Option<PathBuf>>,
    store: LocalStore,
    directory: PathBuf,
    options: ClientOptions,
    sessions: Mutex<Owners>,
    connections: Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<Weak<Remote>>>>>,
    shutdown: tokio::sync::OnceCell<()>,
    transfers: Arc<crate::transfers::runtime::Runtime>,
    cleanup_warned: std::sync::atomic::AtomicBool,
}

#[derive(Default)]
struct Owners {
    closed: bool,
    sessions: Vec<Weak<crate::local::LocalRuntime>>,
}

impl Client {
    pub async fn open(options: ClientOptions) -> Result<Self> {
        let directory = options
            .data_dir
            .clone()
            .map(Ok)
            .unwrap_or_else(crate::store::default_data_dir)?;
        let store = LocalStore::open(&directory).await?;
        let client = Self(Arc::new(State {
            selected_program: Mutex::new(options.codex_program.clone()),
            store,
            directory,
            options,
            sessions: Mutex::new(Owners::default()),
            connections: Mutex::new(std::collections::HashMap::new()),
            shutdown: tokio::sync::OnceCell::new(),
            transfers: Arc::default(),
            cleanup_warned: std::sync::atomic::AtomicBool::new(false),
        }));
        Ok(client)
    }

    pub fn servers(&self) -> ServerService {
        ServerService {
            client: self.clone(),
        }
    }
    pub fn config(&self) -> ConfigService {
        ConfigService {
            client: self.clone(),
        }
    }
    pub fn sessions(&self) -> SessionService {
        SessionService {
            client: self.clone(),
        }
    }
    pub fn workspaces(&self) -> WorkspaceService {
        WorkspaceService {
            client: self.clone(),
        }
    }
    pub fn transfers(&self) -> TransferService {
        TransferService {
            client: self.clone(),
        }
    }
    pub fn native(&self) -> NativeService {
        NativeService {
            client: self.clone(),
        }
    }
    pub async fn select_program(&self, path: PathBuf) -> Result<()> {
        let path = remote_codex_adapter::program::validate(path).await?;
        *self
            .0
            .selected_program
            .lock()
            .map_err(|_| crate::ClientError::RemoteResponse)? = Some(path);
        Ok(())
    }
    pub async fn close(&self) {
        self.0
            .shutdown
            .get_or_init(|| async {
                let sessions = self
                    .0
                    .sessions
                    .lock()
                    .map(|mut owners| {
                        owners.closed = true;
                        std::mem::take(&mut owners.sessions)
                    })
                    .unwrap_or_default();
                for session in sessions {
                    if let Some(session) = session.upgrade() {
                        session.shutdown().await;
                    }
                }
                self.0.transfers.close().await;
                self.0.store.close().await;
            })
            .await;
    }
}

impl State {
    async fn program(&self) -> Result<PathBuf> {
        let selected = self
            .selected_program
            .lock()
            .map_err(|_| crate::ClientError::RemoteResponse)?
            .clone();
        Ok(if self.options.desktop_discovery {
            remote_codex_adapter::program::discover_desktop(selected).await?
        } else if let Some(path) = &selected {
            remote_codex_adapter::program::validate(path.clone()).await?
        } else {
            remote_codex_adapter::program::discover().await?
        })
    }
    fn ensure_open(&self) -> Result<()> {
        if self.sessions.lock().is_ok_and(|owners| !owners.closed) {
            Ok(())
        } else {
            Err(crate::ClientError::Argument("application client is closed"))
        }
    }

    fn register(&self, runtime: &Arc<crate::local::LocalRuntime>) -> Result<()> {
        let mut owners = self
            .sessions
            .lock()
            .map_err(|_| crate::ClientError::RemoteResponse)?;
        if owners.closed {
            return Err(crate::ClientError::Argument("application client is closed"));
        }
        owners.sessions.retain(|session| session.strong_count() > 0);
        owners.sessions.push(Arc::downgrade(runtime));
        Ok(())
    }

    fn progress(&self, event: PrepareEvent) {
        if let Some(callback) = crate::progress::current().or_else(|| self.options.progress.clone())
        {
            callback(event);
        }
    }

    fn bundle(&self) -> crate::servers::ServerBundle<'_> {
        crate::servers::ServerBundle {
            bytes: &self.options.service.bytes,
            sha256: &self.options.service.sha256,
        }
    }

    async fn connect(
        &self,
        record: crate::store::ConnectionRecord,
    ) -> Result<Arc<crate::remote::Remote>> {
        let slot = {
            let mut connections = self
                .connections
                .lock()
                .map_err(|_| crate::ClientError::RemoteResponse)?;
            connections
                .entry(record.id.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(Weak::new())))
                .clone()
        };
        let mut connection = slot.lock().await;
        if let Some(remote) = connection.upgrade() {
            remote.recover(false).await?;
            remote.synchronize(&self.store).await?;
            return Ok(remote);
        }
        let remote = crate::ssh::interaction::scope(
            self.options.authentication.clone(),
            crate::servers::ensure(
                &self.store,
                &self.directory,
                record,
                self.options.ssh_config.clone(),
                self.options.interactive,
                self.bundle(),
                |event| self.progress(event),
            ),
        )
        .await?;
        self.progress(PrepareEvent::Stage(
            crate::progress::PrepareStage::Synchronize,
        ));
        remote.synchronize(&self.store).await?;
        let remote = Arc::new(remote);
        *connection = Arc::downgrade(&remote);
        self.cleanup_credentials().await;
        Ok(remote)
    }
}
