mod driver;

use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    process::Command,
    sync::{broadcast, mpsc, oneshot},
};

pub type Answer = Result<Value, Fault>;
pub type Pending = oneshot::Receiver<Answer>;

/// Native catalogs include plugin assets and can exceed the SSH execution frames.
/// Keep this local transport bounded independently from the remote wire protocol.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

enum CommandMessage {
    Request {
        id: u64,
        method: String,
        params: Value,
        answer: oneshot::Sender<Answer>,
    },
    Send(Value),
    Stop,
}

struct Inner {
    sender: mpsc::Sender<CommandMessage>,
    events: broadcast::Sender<Value>,
    next: AtomicU64,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Clone)]
pub struct Engine(Arc<Inner>);

impl Engine {
    pub async fn local(program: &Path, home: &Path) -> Result<(Self, Value), Fault> {
        let mut command = Command::new(program);
        command.current_dir(home);
        command
            .args(["app-server", "--listen", "stdio://"])
            .env("CODEX_HOME", home)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .process_group(0)
            .kill_on_drop(true);
        let child = command
            .spawn()
            .map_err(|_| Fault::new("ENGINE_START_FAILED", "could not start local Codex"))?;
        let (sender, receiver) = mpsc::channel(64);
        let (events, _) = broadcast::channel(512);
        let task = tokio::spawn(driver::run(child, receiver, events.clone()));
        let engine = Self(Arc::new(Inner {
            sender,
            events,
            next: AtomicU64::new(1),
            task,
        }));
        let initialized = match engine.call("initialize", json!({"clientInfo":{"name":"remote_codex","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}})).await {
            Ok(value) => value,
            Err(error) => {
                engine.shutdown().await;
                return Err(error);
            }
        };
        engine.send(json!({"method":"initialized","params":{}}))?;
        Ok((engine, initialized))
    }

    /// Queue a request without blocking the runtime's notification consumer.
    pub fn begin(&self, method: &str, params: Value) -> Result<Pending, Fault> {
        let id = self
            .0
            .next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| Fault::new("ENGINE_BUSY", "request identity exhausted"))?;
        let (answer, pending) = oneshot::channel();
        self.0
            .sender
            .try_send(CommandMessage::Request {
                id,
                method: method.into(),
                params,
                answer,
            })
            .map_err(|_| {
                Fault::new(
                    "ENGINE_BUSY",
                    "Codex is unavailable or its request queue is full",
                )
            })?;
        Ok(pending)
    }

    pub async fn finish(pending: Pending) -> Answer {
        tokio::time::timeout(Duration::from_secs(30), pending)
            .await
            .map_err(|_| Fault {
                code: "CODEX_RESPONSE_TIMEOUT".into(),
                message: "Codex response timed out; inspect state before retrying".into(),
                outcome_unknown: true,
            })?
            .map_err(|_| Fault::unknown("Codex exited before acknowledging the operation"))?
    }

    pub async fn call(&self, method: &str, params: Value) -> Answer {
        Self::finish(self.begin(method, params)?)
            .await
            .map_err(|mut error| {
                error.message = format!("{method}: {}", error.message);
                error
            })
    }

    pub fn send(&self, value: Value) -> Result<(), Fault> {
        self.0
            .sender
            .try_send(CommandMessage::Send(value))
            .map_err(|_| Fault::unknown("Codex request queue is unavailable"))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.0.events.subscribe()
    }

    pub fn stop(&self) {
        if self.0.sender.try_send(CommandMessage::Stop).is_err() {
            self.0.task.abort();
        }
    }

    /// Keep ownership until the driver has flushed and reaped its child.
    pub async fn shutdown(&self) {
        self.stop();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
        while !self.0.task.is_finished() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        if !self.0.task.is_finished() {
            self.0.task.abort();
        }
    }
}
