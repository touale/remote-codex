//! Application boundary shared by the CLI and future desktop clients.
mod authentication;
mod config;
mod handle;
mod notice;
mod server;
mod session;

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
    store: LocalStore,
    directory: PathBuf,
    options: ClientOptions,
    sessions: Mutex<Owners>,
    connections: Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<Weak<Remote>>>>>,
    shutdown: tokio::sync::OnceCell<()>,
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
            store,
            directory,
            options,
            sessions: Mutex::new(Owners::default()),
            connections: Mutex::new(std::collections::HashMap::new()),
            shutdown: tokio::sync::OnceCell::new(),
            cleanup_warned: std::sync::atomic::AtomicBool::new(false),
        }));
        client.0.cleanup_credentials().await;
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
                self.0.store.close().await;
            })
            .await;
    }
}

impl State {
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
        if let Some(callback) = &self.options.progress {
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
        let remote = crate::servers::ensure(
            &self.store,
            &self.directory,
            record,
            self.options.ssh_config.clone(),
            self.options.interactive,
            self.bundle(),
            |event| self.progress(event),
        )
        .await?;
        self.progress(PrepareEvent::Stage(
            crate::progress::PrepareStage::Synchronize,
        ));
        remote.synchronize(&self.store).await?;
        let remote = Arc::new(remote);
        *connection = Arc::downgrade(&remote);
        Ok(remote)
    }
}
