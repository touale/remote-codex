use super::{LocalRuntime, route};
use crate::{ClientError, Result};
use futures_util::{SinkExt, StreamExt, stream::FuturesUnordered};
use remote_codex_adapter::engine::MAX_MESSAGE_BYTES;
use remote_codex_protocol::Fault;
use serde_json::Value;
use std::{collections::HashSet, path::PathBuf, sync::Arc, time::Duration};
use tokio::{net::UnixListener, sync::watch};
use tokio_tungstenite::{
    accept_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

pub struct Gateway {
    pub socket: PathBuf,
    pub closed: watch::Receiver<bool>,
    task: tokio::task::JoinHandle<Result<()>>,
    _directory: tempfile::TempDir,
}

impl Gateway {
    pub async fn start(runtime: Arc<LocalRuntime>) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("rc-ui-")
            .tempdir_in("/tmp")?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
        crate::store::private_directory(directory.path())?;
        let socket = directory.path().join("frontend.sock");
        let listener = UnixListener::bind(&socket)?;
        let (closed, receiver) = watch::channel(false);
        let task = tokio::spawn(async move {
            let result = serve(listener, &runtime).await;
            if result.is_err() {
                let _ = closed.send(true);
            }
            runtime.shutdown().await;
            let _ = closed.send(true);
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
        match tokio::time::timeout(Duration::from_secs(10), &mut self.task).await {
            Ok(result) => result.map_err(|_| ClientError::RemoteResponse)?,
            Err(_) => Err(ClientError::Timeout),
        }
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(listener: UnixListener, runtime: &Arc<LocalRuntime>) -> Result<()> {
    let (stream, _) = tokio::time::timeout(Duration::from_secs(30), listener.accept())
        .await
        .map_err(|_| ClientError::Timeout)??;
    if stream.peer_cred()?.uid() != nix::unistd::geteuid().as_raw() {
        return Err(ClientError::PrivatePath);
    }
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    let mut frontend = tokio::time::timeout(
        Duration::from_secs(5),
        accept_async_with_config(stream, Some(config)),
    )
    .await
    .map_err(|_| ClientError::Timeout)?
    .map_err(|_| ClientError::RemoteResponse)?;
    let mut events = runtime.engine.subscribe();
    let mut revoked = runtime.lease.revoked.clone();
    let mut pending = FuturesUnordered::new();
    let mut approvals = HashSet::new();
    loop {
        tokio::select! {
            _ = revoked.changed() => break,
            message = frontend.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text)?;
                    if let Some(method) = value["method"].as_str() {
                        if method == "initialized" {continue;}
                        let Some(id) = value.get("id").cloned() else {continue;};
                        if pending.len() >= 64 {return Err(ClientError::Argument("too many pending frontend requests"));}
                        let runtime = runtime.clone(); let method = method.to_owned();
                        pending.push(async move { route::response(id, route::request(&runtime, &method, value.get("params").cloned().unwrap_or_else(||serde_json::json!({}))).await) });
                    } else if let Some(id) = value.get("id") {
                        if !approvals.remove(&id.to_string()) {return Err(Fault::new("STALE_APPROVAL", "approval is no longer pending").into());}
                        runtime.approvals.respond(&value)?;
                        runtime.engine.send(value)?;
                    }
                }
                Some(Ok(Message::Ping(_))) => {frontend.flush().await.map_err(|_|ClientError::RemoteResponse)?;}
                Some(Ok(Message::Pong(_))) => {},
                Some(Ok(Message::Close(_))) | None => break,
                _ => return Err(ClientError::RemoteResponse),
            },
            Some(response) = pending.next(), if !pending.is_empty() => {
                frontend.send(Message::Text(response.to_string().into())).await.map_err(|_|ClientError::RemoteResponse)?;
            },
            event = events.recv() => {
                let mut event = event.map_err(|_|ClientError::RemoteResponse)?;
                if event["method"] == "remoteCodex/engineClosed" {
                    return match event.pointer("/params/fault").filter(|v| !v.is_null()) {
                        Some(fault) => Err(serde_json::from_value::<Fault>(fault.clone())?.into()),
                        None => Ok(()),
                    };
                }
                if let Some(response)=super::recovery::handle(runtime,&event).await {runtime.engine.send(response)?;continue;}
                runtime.approvals.observe(&mut event, &runtime.binding, &runtime.bridge.channel)?;
                runtime.permissions.observe(&event)?;
                if let Some(id) = event.get("id") {approvals.insert(id.to_string());}
                if approvals.len() > 64 {return Err(ClientError::Argument("too many pending approval requests"));}
                frontend.send(Message::Text(event.to_string().into())).await.map_err(|_|ClientError::RemoteResponse)?;
            }
        }
    }
    Ok(())
}
