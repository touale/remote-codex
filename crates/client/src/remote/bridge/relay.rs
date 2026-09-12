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
    recovery: &super::super::recovery::Recovery,
) -> Result<()> {
    let mut cursor = 0;
    let mut backend = attach(remote, id, cursor).await?;
    let mut outbox = BTreeMap::<String, Frame>::new();
    let mut heard = tokio::time::Instant::now();
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
                    heard = tokio::time::Instant::now();
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
                Ok(Some(Frame::Heartbeat)) => { heard = tokio::time::Instant::now(); false },
                Ok(Some(Frame::Error(fault))) => return Err(fault.into()),
                Ok(None) | Err(_) => true,
                _ => return Err(ClientError::RemoteResponse),
            },
            _ = heartbeat.tick() => !remote.ssh.healthy() || heard.elapsed() > Duration::from_secs(20) || codec::write(&mut backend.writer, &Frame::Heartbeat).await.is_err(),
        };
        if reconnect {
            let mut attempt = 1;
            loop {
                let restored = tokio::select! {
                    result = reconnect_channel(remote, id, cursor, recovery, attempt) => result,
                    _ = stopped.changed() => return Ok(()),
                };
                backend = restored?;
                let mut delivered = true;
                for frame in outbox.values() {
                    if codec::write(&mut backend.writer, frame).await.is_err() {
                        delivered = false;
                        break;
                    }
                }
                if delivered {
                    heard = tokio::time::Instant::now();
                    recovery.publish(remote_codex_core::session::EnvironmentState::Ready);
                    break;
                }
                attempt = attempt.saturating_add(1);
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
    tokio::time::timeout(
        Duration::from_secs(15),
        stream.call(
            &remote.profile,
            Request::AttachExecution {
                channel: channel.into(),
                after,
            },
        ),
    )
    .await
    .map_err(|_| ClientError::Timeout)??;
    Ok(stream)
}

async fn reconnect_channel(
    remote: &Remote,
    id: &str,
    after: i64,
    recovery: &super::super::recovery::Recovery,
    mut attempt: u32,
) -> Result<Channel> {
    let mut error = ClientError::Ssh(255);
    loop {
        recovery.wait(attempt, &error).await;
        let result = async {
            remote.recover(false).await?;
            let status: remote_codex_protocol::ExecutionStatus = serde_json::from_value(
                remote
                    .call(Request::InspectExecution { channel: id.into() })
                    .await?,
            )?;
            if status.state != "running" || after < status.replay_floor {
                return Err(remote_codex_protocol::Fault {
                    code: "EXECUTION_LOST".into(),
                    message: status
                        .reason
                        .unwrap_or_else(|| "execution replay expired".into()),
                    outcome_unknown: true,
                }
                .into());
            }
            attach(remote, id, after).await
        }
        .await;
        match result {
            Ok(channel) => return Ok(channel),
            Err(fault)
                if matches!(
                    fault.code(),
                    "EXECUTION_LOST" | "EXECUTION_REPLAY_EXPIRED" | "OPERATION_OUTCOME_UNKNOWN"
                ) =>
            {
                return Err(fault);
            }
            Err(fault) => error = fault,
        }
        attempt = attempt.saturating_add(1);
    }
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
