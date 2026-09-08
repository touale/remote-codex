use super::*;
use crate::remote::Channel;
use futures_util::{SinkExt, StreamExt};
use remote_codex_protocol::{Frame, codec};
use serde_json::Value;
use std::collections::BTreeMap;
use tokio::net::TcpStream;
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};

pub(super) async fn run(
    mut frontend: WebSocketStream<TcpStream>,
    remote: &Remote,
    id: &str,
    stopped: &mut watch::Receiver<bool>,
    approvals: &crate::local::approvals::Approvals,
    permissions: &crate::local::permissions::Permissions,
) -> Result<()> {
    let mut cursor = 0;
    let mut backend = attach(remote, id, cursor).await?;
    let mut outbox = BTreeMap::<String, Frame>::new();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
    loop {
        let reconnect = tokio::select! {
            _ = stopped.changed() => break,
            incoming = frontend.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if outbox.len() >= 64 { return Err(ClientError::Argument("execution queue is full")); }
                    let message: Value = serde_json::from_str(&text)?;
                    if forwards_account_credentials(&message) {
                        let response=serde_json::json!({"id":message["id"],"error":{"code":-32000,"message":"Codex account credentials must stay local; this execution request was not sent to the server"}});
                        frontend.send(Message::Text(response.to_string().into())).await.map_err(|_|ClientError::RemoteResponse)?;
                        continue;
                    }
                    let approval = approvals.take(&message)?.map(Box::new);
                    let permissions = if approval.is_none() { permissions.authorization(&message)?.map(Box::new) } else { None };
                    let required = if approval.is_some() { Some(remote_codex_protocol::COMMAND_APPROVAL_CAPABILITY) } else if permissions.is_some() { Some(remote_codex_protocol::SESSION_PERMISSIONS_CAPABILITY) } else { None };
                    if required.is_some_and(|required| !remote.identity.capabilities.iter().any(|c| c == required)) {
                        let response=serde_json::json!({"id":message["id"],"error":{"code":-32000,"message":"SERVICE_UPDATE_REQUIRED: the remote service must be updated to honor these permissions; no operation was run"}});
                        frontend.send(Message::Text(response.to_string().into())).await.map_err(|_|ClientError::RemoteResponse)?;
                        continue;
                    }
                    let operation = approval.as_ref().map(|a| a.id.clone()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let frame = Frame::Execute {operation:operation.clone(), message, approval, permissions};
                    outbox.insert(operation, frame.clone());
                    codec::write(&mut backend.writer, &frame).await.is_err()
                }
                Some(Ok(Message::Ping(_))) => { frontend.flush().await.map_err(|_| ClientError::RemoteResponse)?; false }
                Some(Ok(Message::Pong(_))) => false,
                Some(Ok(Message::Close(_))) | None => break,
                _ => return Err(ClientError::RemoteResponse),
            },
            frame = backend.reader.next::<Frame>() => match frame {
                Ok(Some(Frame::Event {cursor: next, message})) => {
                    if next > cursor {
                        if message["method"] == "remoteCodex/operationAccepted" {
                            if let Some(operation) = message.pointer("/params/operation").and_then(Value::as_str) { outbox.remove(operation); }
                        } else {
                            frontend.send(Message::Text(message.to_string().into())).await.map_err(|_| ClientError::RemoteResponse)?;
                        }
                        cursor = next;
                    }
                    codec::write(&mut backend.writer, &Frame::Ack {cursor}).await.is_err()
                }
                Ok(Some(Frame::Error(fault))) => return Err(fault.into()),
                Ok(None) | Err(_) => true,
                _ => return Err(ClientError::RemoteResponse),
            },
            _ = heartbeat.tick() => codec::write(&mut backend.writer, &Frame::Heartbeat).await.is_err(),
        };
        if reconnect {
            backend = tokio::select! {
                result = reconnect_channel(remote, id, cursor) => result?,
                _ = stopped.changed() => break,
            };
            for frame in outbox.values() {
                codec::write(&mut backend.writer, frame).await?;
            }
        }
    }
    let _ = codec::write(&mut backend.writer, &Frame::Detach).await;
    Ok(())
}

fn forwards_account_credentials(message: &Value) -> bool {
    ["/params/env", "/params/envPolicy/set"]
        .into_iter()
        .filter_map(|p| message.pointer(p).and_then(Value::as_object))
        .any(|env| {
            env.keys().any(|key| {
                matches!(
                    key.to_ascii_uppercase().as_str(),
                    "OPENAI_API_KEY" | "CODEX_API_KEY" | "CODEX_ACCESS_TOKEN" | "CODEX_AUTH_JSON"
                )
            })
        })
}

async fn attach(remote: &Remote, channel: &str, after: i64) -> Result<Channel> {
    let mut stream = remote.channel().await?;
    stream
        .call(
            &remote.profile,
            Request::AttachExecution {
                channel: channel.into(),
                after,
            },
        )
        .await?;
    Ok(stream)
}

async fn reconnect_channel(remote: &Remote, id: &str, after: i64) -> Result<Channel> {
    for delay in [1, 2, 4, 8] {
        tokio::time::sleep(Duration::from_secs(delay)).await;
        match tokio::time::timeout(Duration::from_secs(15), attach(remote, id, after)).await {
            Ok(Ok(channel)) => return Ok(channel),
            Ok(Err(error @ ClientError::RemoteFault(..))) => return Err(error),
            _ => {}
        }
    }
    Err(ClientError::RemoteFault("RECONNECT_FAILED".into(), "execution connection could not be restored; resume the local session to inspect jobs before retrying".into(), true))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_credentials_are_rejected_before_ssh_serialization() {
        for path in ["env", "envPolicy"] {
            let mut message = serde_json::json!({"params":{}});
            message["params"][path] = if path == "env" {
                serde_json::json!({"OPENAI_API_KEY":"never-send"})
            } else {
                serde_json::json!({"set":{"CODEX_ACCESS_TOKEN":"never-send"}})
            };
            assert!(forwards_account_credentials(&message));
        }
        assert!(!forwards_account_credentials(
            &serde_json::json!({"params":{"env":{"LANG":"en_US.UTF-8"}}})
        ));
    }
}
