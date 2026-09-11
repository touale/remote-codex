use crate::{ClientError, Result};
use remote_codex_core::session::EnvironmentState;
use std::time::Duration;
use tokio::sync::{Notify, watch};

pub(crate) struct Recovery {
    pub(crate) state: watch::Sender<EnvironmentState>,
    pub(crate) retry: Notify,
}

impl Default for Recovery {
    fn default() -> Self {
        Self {
            state: watch::channel(EnvironmentState::Ready).0,
            retry: Notify::new(),
        }
    }
}

impl Recovery {
    pub(crate) fn publish(&self, state: EnvironmentState) {
        self.state.send_if_modified(|current| {
            if *current == EnvironmentState::Closed || *current == state {
                return false;
            }
            *current = state;
            true
        });
    }

    pub(crate) fn ready(&self) -> Result<()> {
        match &*self.state.borrow() {
            EnvironmentState::Ready => Ok(()),
            EnvironmentState::ActionRequired { code, message } => {
                Err(remote_codex_protocol::Fault::new(code, message).into())
            }
            _ => Err(remote_codex_protocol::Fault::new("ENVIRONMENT_NOT_READY", "execution environment is reconnecting or requires attention; no new turn was submitted").into()),
        }
    }

    pub(crate) async fn wait(&self, attempt: u32, error: &ClientError) {
        if transient(error) {
            let base = (1u64 << attempt.saturating_sub(1).min(5)).min(30) * 1000;
            let jitter = u64::from(uuid::Uuid::new_v4().as_bytes()[0]) * base / 2550;
            let delay = (base.saturating_sub(jitter)).max(1);
            self.publish(EnvironmentState::Reconnecting {
                attempt,
                retry_in_ms: delay,
            });
            tokio::select! { _=tokio::time::sleep(Duration::from_millis(delay))=>{}, _=self.retry.notified()=>{} }
        } else {
            self.publish(EnvironmentState::ActionRequired {
                code: error.code().into(),
                message: error.to_string(),
            });
            self.retry.notified().await;
        }
    }
}

pub(crate) fn transient(error: &ClientError) -> bool {
    matches!(
        error,
        ClientError::Ssh(255) | ClientError::Timeout | ClientError::RemoteResponse
    ) || matches!(error, ClientError::Io(e) if matches!(e.kind(), std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::TimedOut))
        || matches!(error, ClientError::RemoteFault(code, _, _) if matches!(code.as_str(), "SERVICE_UPDATE_BUSY" | "EXECUTION_BUSY" | "CAPACITY_EXCEEDED"))
}
