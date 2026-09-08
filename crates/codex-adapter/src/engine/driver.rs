use super::{Answer, CommandMessage, MAX_MESSAGE_BYTES};
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use remote_codex_protocol::{Fault, codec};
use serde_json::{Value, json};
use std::collections::HashMap;
use tokio::{
    io::BufReader,
    process::Child,
    sync::{broadcast, mpsc, oneshot},
};

struct Group(Option<Pid>);
impl Drop for Group {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            let _ = killpg(pid, Signal::SIGKILL);
        }
    }
}

pub(super) async fn run(
    mut child: Child,
    mut commands: mpsc::Receiver<CommandMessage>,
    events: broadcast::Sender<Value>,
) {
    let _group = Group(
        child
            .id()
            .and_then(|v| i32::try_from(v).ok())
            .map(Pid::from_raw),
    );
    let Some(mut stdin) = child.stdin.take() else {
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        return;
    };
    let mut stdout = codec::Reader::with_limit(BufReader::new(stdout), MAX_MESSAGE_BYTES);
    let mut pending: HashMap<u64, oneshot::Sender<Answer>> = HashMap::new();
    let mut failure = None;
    let mut expected_stop = false;
    loop {
        tokio::select! {
            command = commands.recv() => {
                let value = match command {
                    Some(CommandMessage::Request { id, method, params, answer }) => {
                        pending.retain(|_, tx| !tx.is_closed());
                        if pending.len() >= 64 {
                            let _ = answer.send(Err(Fault::new("ENGINE_BUSY", "too many pending Codex requests")));
                            continue;
                        }
                        pending.insert(id, answer);
                        json!({"id":id,"method":method,"params":params})
                    }
                    Some(CommandMessage::Send(value)) => value,
                    Some(CommandMessage::Stop) | None => { expected_stop = true; break; },
                };
                if let Err(error) = codec::write_with_limit(&mut stdin, &value, MAX_MESSAGE_BYTES).await {
                    failure = Some(protocol_failure("write", &error)); break;
                }
            }
            message = stdout.next::<Value>() => {
                let value = match message {
                    Ok(Some(value)) => value,
                    Ok(None) => break,
                    Err(error) => { failure = Some(protocol_failure("read", &error)); break; },
                };
                if value.get("method").is_some() {
                    let _ = events.send(value);
                } else if let Some(id) = value.get("id").and_then(Value::as_u64) {
                    if let Some(answer) = pending.remove(&id) {
                        let result = match (value.get("result"), value.get("error")) {
                            (Some(result), None) => Ok(result.clone()),
                            (None, Some(error)) => {
                                let message = error["message"].as_str().unwrap_or("Codex rejected the operation").to_owned();
                                let message:String=message.chars().filter(|c|!c.is_control()).take(1000).collect();
                                Err(Fault::new("CODEX_REQUEST_FAILED", &message))
                            },
                            _ => Err(Fault::unknown("invalid Codex response")),
                        };
                        let _ = answer.send(result);
                    }
                } else { failure = Some(Fault::unknown("local Codex sent an invalid RPC envelope")); break; }
            }
        }
    }
    // Closing stdin lets Codex flush its rollout and database before exit.
    drop(stdin);
    let status = match tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await {
        Ok(Ok(status)) => Some(status),
        _ => {
            let _ = child.kill().await;
            None
        }
    };
    if !expected_stop && failure.is_none() {
        let detail = status.map_or_else(|| "exit status unavailable".into(), |s| s.to_string());
        failure = Some(Fault::unknown(&format!(
            "local Codex exited unexpectedly ({detail})"
        )));
    }
    let _ = events.send(json!({"method":"remoteCodex/engineClosed","params":{"fault":failure}}));
    for (_, answer) in pending {
        let _ = answer.send(Err(failure.clone().unwrap_or_else(|| {
            Fault::unknown("local Codex stopped before acknowledging the request")
        })));
    }
}

fn protocol_failure(operation: &str, error: &std::io::Error) -> Fault {
    Fault {
        code: "CODEX_PROTOCOL_ERROR".into(),
        message: format!("local Codex protocol {operation} failed: {error}"),
        outcome_unknown: true,
    }
}
