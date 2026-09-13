mod actor;
mod operations;
mod policy;
mod runtime;

use crate::{Result, config::Runtime, paths, storage::Store};
use remote_codex_adapter::executor::Executor;
use remote_codex_protocol::Fault;
use serde_json::Value;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use tokio::sync::{Notify, mpsc};

pub(crate) enum Input {
    Execute {
        operation: String,
        message: Value,
        approval: Option<Box<remote_codex_protocol::CommandApproval>>,
        permissions: Option<Box<remote_codex_protocol::SessionPermissions>>,
    },
    Touch,
    Detached,
    RetireIdle,
    Disconnected,
    Stop,
}

pub(crate) struct Execution {
    pub(crate) runtime: remote_codex_protocol::ExecutionRuntime,
    sender: mpsc::Sender<Input>,
    pub(crate) changed: Notify,
    pub(crate) alive: AtomicBool,
    pub(crate) epoch: AtomicU64,
}

impl Execution {
    pub(crate) async fn start(
        root: &Path,
        id: &str,
        profile: &str,
        runtime: Runtime,
        selected: &remote_codex_protocol::ExecutionRuntime,
        updates: tokio::sync::watch::Receiver<crate::profiles::LiveSettings>,
        store: Store,
    ) -> Result<Arc<Self>> {
        let home = root.join("executors").join(id);
        paths::private(&home)?;
        let program = runtime::resolve(root, selected).await?;
        let backend = Executor::start(&program, &home, &runtime.env).await?;
        store
            .create_channel(id, profile, runtime.config.revision)
            .await?;
        let (sender, receiver) = mpsc::channel(64);
        let execution = Arc::new(Self {
            runtime: selected.clone(),
            sender,
            changed: Notify::new(),
            alive: AtomicBool::new(true),
            epoch: AtomicU64::new(0),
        });
        let weak = Arc::downgrade(&execution);
        let id = id.to_owned();
        let profile = profile.to_owned();
        tokio::spawn(async move {
            actor::run(
                (&id, &profile),
                runtime,
                updates,
                store,
                backend,
                receiver,
                &weak,
            )
            .await;
            if let Some(handle) = weak.upgrade() {
                handle.alive.store(false, Ordering::Release);
                handle.changed.notify_waiters();
            }
        });
        Ok(execution)
    }
    pub(crate) fn send(&self, input: Input) -> Result<()> {
        if !self.alive.load(Ordering::Acquire) {
            return Err(Fault::unknown(
                "execution backend stopped; open a new local runtime to continue",
            ));
        }
        self.sender
            .try_send(input)
            .map_err(|_| Fault::new("EXECUTION_BUSY", "execution request queue is full"))
    }
}

pub(crate) struct Attachment(pub(crate) Arc<Execution>, pub(crate) u64);
impl Drop for Attachment {
    fn drop(&mut self) {
        if self.0.epoch.load(Ordering::Acquire) == self.1 {
            let _ = self.0.send(Input::Disconnected);
        }
    }
}
