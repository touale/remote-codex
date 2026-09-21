//! Native TUI transport. Product authorization and persistence live in the backend.
mod connection;

use crate::thread::response;
use connection::Connection;
use futures_util::{StreamExt, stream::FuturesUnordered};
use remote_codex_protocol::Fault;
use serde_json::Value;
use std::{future::Future, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    net::UnixListener,
    sync::{broadcast, watch},
};
use tokio_tungstenite::tungstenite::Message;

// Local frontend attachment is separate from configurable SSH recovery attempts.
const ATTACH_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PENDING_REQUESTS: usize = 64;

type Result<T> = std::result::Result<T, Fault>;

pub trait Backend: Send + Sync + 'static {
    fn request(&self, method: &str, params: Value) -> impl Future<Output = Result<Value>> + Send;
    fn respond(&self, response: Value) -> Result<()>;
    fn events(&self) -> broadcast::Receiver<Value>;
    fn pending_requests(&self) -> Result<Vec<Value>>;
    fn revoked(&self) -> watch::Receiver<bool>;
    fn closed(&self) -> watch::Receiver<Option<Option<String>>>;
    fn close(&self) -> impl Future<Output = ()> + Send;
}

pub struct Gateway {
    pub socket: PathBuf,
    pub closed: watch::Receiver<bool>,
    task: tokio::task::JoinHandle<Result<()>>,
    stop: watch::Sender<bool>,
    _directory: tempfile::TempDir,
}

impl Gateway {
    pub async fn start<B: Backend>(backend: Arc<B>) -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::Builder::new()
            .prefix("rc-ui-")
            .tempdir_in("/tmp")
            .map_err(transport)?;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
            .map_err(transport)?;
        let socket = directory.path().join("frontend.sock");
        let listener = UnixListener::bind(&socket).map_err(transport)?;
        let (closed, receiver) = watch::channel(false);
        let (stop, stopped) = watch::channel(false);
        let task = tokio::spawn(async move {
            let result = serve(listener, &backend, stopped).await;
            if result.is_err() {
                closed.send_replace(true);
            }
            backend.close().await;
            closed.send_replace(true);
            result
        });
        Ok(Self {
            socket,
            closed: receiver,
            task,
            stop,
            _directory: directory,
        })
    }

    pub async fn finish(mut self) -> Result<()> {
        self.stop.send_replace(true);
        tokio::time::timeout(SHUTDOWN_TIMEOUT, &mut self.task)
            .await
            .map_err(transport)?
            .map_err(transport)?
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve<B: Backend>(
    listener: UnixListener,
    backend: &Arc<B>,
    mut stopped: watch::Receiver<bool>,
) -> Result<()> {
    let mut events = backend.events();
    let mut revoked = backend.revoked();
    let mut closed = backend.closed();
    if *revoked.borrow() || closed.borrow().is_some() {
        return Err(Fault::new("SESSION_CLOSED", "session is closed"));
    }
    let mut frontend: Option<Connection> = None;
    let mut epoch = 0_u64;
    // Requests already dispatched to Codex survive a frontend disconnect. Their
    // replies belong only to the originating socket, never to reused RPC IDs.
    let mut pending = FuturesUnordered::new();
    let mut handshakes = FuturesUnordered::new();
    let mut deadline = tokio::time::Instant::now() + ATTACH_TIMEOUT;
    let mut last_error = None;
    loop {
        tokio::select! {
            _ = stopped.changed() => return Ok(()),
            _ = revoked.changed() => return Ok(()),
            _ = closed.changed() => return closed_result(&closed),
            _ = tokio::time::sleep_until(deadline), if frontend.is_none() => {
                let detail = last_error.map_or_else(String::new, |error: Fault| format!(": {error}"));
                return Err(transport(format!("native frontend did not connect within {} seconds{detail}", ATTACH_TIMEOUT.as_secs())));
            }
            accepted = listener.accept(), if handshakes.is_empty() => {
                let (stream, _) = accepted.map_err(transport)?;
                // Negotiate one replacement without pausing the active connection.
                handshakes.push(Connection::accept(stream));
            }
            Some(next) = handshakes.next(), if !handshakes.is_empty() => {
                match next {
                    Ok(next) => {
                        epoch = epoch.wrapping_add(1);
                        frontend = Some(next);
                        last_error = None;
                    }
                    Err(error) if frontend.is_none() => last_error = Some(error),
                    Err(_) => {}, // A rejected candidate must not disrupt a healthy frontend.
                }
            }
            message = async {
                match &mut frontend {
                    Some(connection) => connection.next().await,
                    None => std::future::pending().await,
                }
            } => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let value: Value = serde_json::from_str(&text).map_err(transport)?;
                        if let Some(method) = value["method"].as_str() {
                            if method == "initialized" { continue; }
                            let Some(id) = value.get("id").cloned() else { continue; };
                            if pending.len() >= MAX_PENDING_REQUESTS { return Err(Fault::new("FRONTEND_BUSY", "too many pending frontend requests")); }
                            let backend = backend.clone();
                            let method = method.to_owned();
                            let request_epoch = epoch;
                            pending.push(async move {
                                let result = backend.request(&method, value.get("params").cloned().unwrap_or_else(|| serde_json::json!({}))).await;
                                let resumed = method == "thread/resume" && result.is_ok();
                                (request_epoch, response(id, result), resumed)
                            });
                        } else if value.get("id").is_some() { backend.respond(value)?; }
                    }
                    Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {},
                    other => {
                        last_error = Some(match other {
                            Some(Err(error)) => error,
                            _ => transport("native frontend connection closed"),
                        });
                        frontend = None;
                        deadline = tokio::time::Instant::now() + ATTACH_TIMEOUT;
                    }
                }
            }
            Some((request_epoch, response, resumed)) = pending.next(), if !pending.is_empty() => {
                if request_epoch == epoch && let Some(connection) = &mut frontend {
                    connection.send(response);
                    if resumed {
                        // History hydration does not replay outstanding approval prompts.
                        for request in backend.pending_requests()? {
                            connection.send(request);
                        }
                    }
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if let Some(connection) = &mut frontend {
                            connection.send(event);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        // Start one reattachment window; more backlog while disconnected
                        // must not keep extending it indefinitely.
                        if frontend.take().is_some() {
                            deadline = tokio::time::Instant::now() + ATTACH_TIMEOUT;
                            last_error = Some(transport(format!("native frontend missed {skipped} events")));
                        }
                    }
                    Err(error) => return Err(transport(error)),
                }
            }
        }
    }
}

fn closed_result(closed: &watch::Receiver<Option<Option<String>>>) -> Result<()> {
    match &*closed.borrow() {
        Some(Some(reason)) => Err(Fault::new("SESSION_CLOSED", reason)),
        _ => Ok(()),
    }
}

fn transport(error: impl std::fmt::Display) -> Fault {
    Fault::new("FRONTEND_TRANSPORT", &error.to_string())
}
