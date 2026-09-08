//! Native TUI transport. Product authorization and persistence live in the backend.
use crate::{engine::MAX_MESSAGE_BYTES, thread::response};
use futures_util::{SinkExt, StreamExt, stream::FuturesUnordered};
use remote_codex_protocol::Fault;
use serde_json::Value;
use std::{future::Future, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    net::UnixListener,
    sync::{broadcast, watch},
};
use tokio_tungstenite::{
    accept_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

type Result<T> = std::result::Result<T, Fault>;

pub trait Backend: Send + Sync + 'static {
    fn request(&self, method: &str, params: Value) -> impl Future<Output = Result<Value>> + Send;
    fn respond(&self, response: Value) -> Result<()>;
    fn events(&self) -> broadcast::Receiver<Value>;
    fn revoked(&self) -> watch::Receiver<bool>;
    fn closed(&self) -> watch::Receiver<Option<Option<String>>>;
    fn close(&self) -> impl Future<Output = ()> + Send;
}

pub struct Gateway {
    pub socket: PathBuf,
    pub closed: watch::Receiver<bool>,
    task: tokio::task::JoinHandle<Result<()>>,
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
        let task = tokio::spawn(async move {
            let result = serve(listener, &backend).await;
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
            _directory: directory,
        })
    }

    pub async fn finish(mut self) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(10), &mut self.task)
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

async fn serve<B: Backend>(listener: UnixListener, backend: &Arc<B>) -> Result<()> {
    let mut events = backend.events();
    let mut revoked = backend.revoked();
    let mut closed = backend.closed();
    if *revoked.borrow() || closed.borrow().is_some() {
        return Err(Fault::new("SESSION_CLOSED", "session is closed"));
    }
    let (stream, _) = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(30), listener.accept()) => result.map_err(transport)?.map_err(transport)?,
        _ = revoked.changed() => return Ok(()),
        _ = closed.changed() => return closed_result(&closed),
    };
    if stream.peer_cred().map_err(transport)?.uid() != nix::unistd::geteuid().as_raw() {
        return Err(Fault::new(
            "FRONTEND_OWNER",
            "frontend must belong to the current user",
        ));
    }
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    let mut frontend = tokio::time::timeout(
        Duration::from_secs(5),
        accept_async_with_config(stream, Some(config)),
    )
    .await
    .map_err(transport)?
    .map_err(transport)?;
    let mut pending = FuturesUnordered::new();
    loop {
        tokio::select! {
            _ = revoked.changed() => break,
            _ = closed.changed() => return closed_result(&closed),
            message = frontend.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text).map_err(transport)?;
                    if let Some(method) = value["method"].as_str() {
                        if method == "initialized" { continue; }
                        let Some(id) = value.get("id").cloned() else { continue; };
                        if pending.len() >= 64 { return Err(Fault::new("FRONTEND_BUSY", "too many pending frontend requests")); }
                        let backend = backend.clone();
                        let method = method.to_owned();
                        pending.push(async move {
                            response(id, backend.request(&method, value.get("params").cloned().unwrap_or_else(|| serde_json::json!({}))).await)
                        });
                    } else if value.get("id").is_some() { backend.respond(value)?; }
                }
                Some(Ok(Message::Ping(_))) => { frontend.flush().await.map_err(transport)?; }
                Some(Ok(Message::Pong(_))) => {},
                Some(Ok(Message::Close(_))) | None => break,
                _ => return Err(Fault::new("FRONTEND_TRANSPORT", "native frontend connection failed")),
            },
            Some(response) = pending.next(), if !pending.is_empty() => {
                frontend.send(Message::Text(response.to_string().into())).await.map_err(transport)?;
            },
            event = events.recv() => {
                let event = event.map_err(transport)?;
                frontend.send(Message::Text(event.to_string().into())).await.map_err(transport)?;
            }
        }
    }
    Ok(())
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
