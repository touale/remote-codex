use crate::{
    ClientError, Result,
    config::{ConfigKey, ConfigValue},
    store::LocalStore,
};
use remote_codex_core::session::EnvironmentState;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, watch};

const RETRY_DELAY_MS: u64 = 5_000;

pub(crate) struct Recovery {
    pub(crate) state: watch::Sender<EnvironmentState>,
    retry: Notify,
    store: LocalStore,
    server: String,
    attempts: Mutex<Option<Attempts>>,
    rebuilding: AtomicBool,
}

struct Attempts {
    used: u32,
    limit: u16,
}

impl Recovery {
    pub(crate) fn new(store: LocalStore, server: String) -> Self {
        Self {
            state: watch::channel(EnvironmentState::Ready).0,
            retry: Notify::new(),
            store,
            server,
            attempts: Mutex::new(None),
            rebuilding: AtomicBool::new(false),
        }
    }

    pub(crate) fn publish(&self, state: EnvironmentState) {
        self.state.send_if_modified(|current| {
            if *current == EnvironmentState::Closed || *current == state {
                return false;
            }
            if state == EnvironmentState::Ready {
                self.reset_attempts();
                self.rebuilding.store(false, Ordering::Release);
            }
            *current = state;
            true
        });
    }

    pub(crate) fn begin_rebuild(&self) {
        self.rebuilding.store(true, Ordering::Release);
    }

    pub(crate) fn rebuilding(&self) -> bool {
        self.rebuilding.load(Ordering::Acquire)
    }

    pub(crate) fn retry(&self) {
        // Wake the current wait without storing a permit for a later outage.
        self.retry.notify_waiters();
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

    /// Both transport reattachment and generation replacement consume this budget.
    /// A replacement's bridge must fail back to its owner instead of retrying in parallel.
    pub(crate) async fn wait(&self, error: &ClientError) -> Result<()> {
        // Register before publishing the state that enables the Retry button.
        let retried = self.retry.notified();
        let (used, limit) = self.attempts().await?;
        // Bridge transport errors arrive as Fault codes. Recover them and native
        // timeouts only after closing the failed generation, without replaying requests.
        let recoverable = transient(error)
            || (self.rebuilding()
                && matches!(
                    error.code(),
                    "INVALID_REMOTE_RESPONSE"
                        | "CODEX_RESPONSE_TIMEOUT"
                        | "CODEX_CHANGED"
                        | "RUNTIME_UNAVAILABLE"
                        | "RECOVERY_RETRIES_EXHAUSTED"
                ));
        let manual = if !recoverable || used >= u32::from(limit) {
            let (code, message) = if let ClientError::RemoteFault(code, message, _) = error
                && code == "RECOVERY_RETRIES_EXHAUSTED"
            {
                (code.as_str(), message.clone())
            } else if recoverable {
                let noun = if limit == 1 { "attempt" } else { "attempts" };
                (
                    "RECOVERY_RETRIES_EXHAUSTED",
                    format!(
                        "Connection failed after {limit} {noun}. Send a message to try again. Last error: {error}"
                    ),
                )
            } else {
                (error.code(), error.to_string())
            };
            // The transport cannot park with a native tool call still waiting.
            // Let the session owner stop that backend before exposing Retry.
            if !self.rebuilding() {
                return Err(remote_codex_protocol::Fault::new(code, &message).into());
            }
            self.publish(EnvironmentState::ActionRequired {
                code: code.into(),
                message,
            });
            retried.await;
            true
        } else {
            self.publish(EnvironmentState::Reconnecting {
                attempt: used + 1,
                max_attempts: limit,
                retry_in_ms: RETRY_DELAY_MS,
            });
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)) => false,
                _ = retried => true,
            }
        };
        if manual {
            self.reset_attempts();
            self.attempts().await?;
        }
        let (attempt, max_attempts) = {
            let mut attempts = self
                .attempts
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            let attempts = attempts.as_mut().ok_or(ClientError::RemoteResponse)?;
            attempts.used += 1;
            (attempts.used, attempts.limit)
        };
        self.publish(EnvironmentState::Reconnecting {
            attempt,
            max_attempts,
            retry_in_ms: 0,
        });
        Ok(())
    }

    fn reset_attempts(&self) {
        if let Ok(mut attempts) = self.attempts.lock() {
            *attempts = None;
        }
    }

    async fn attempts(&self) -> Result<(u32, u16)> {
        if let Some(attempts) = self
            .attempts
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .as_ref()
        {
            return Ok((attempts.used, attempts.limit));
        }
        let config = self.store.config_snapshot(&self.server).await?;
        let Some(ConfigValue::Integer(limit)) =
            config.effective.get(&ConfigKey::ReconnectMaxAttempts)
        else {
            return Err(ClientError::RemoteResponse);
        };
        let mut attempts = self
            .attempts
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?;
        let attempts = attempts.get_or_insert(Attempts {
            used: 0,
            limit: *limit,
        });
        Ok((attempts.used, attempts.limit))
    }
}

pub(crate) fn transient(error: &ClientError) -> bool {
    matches!(
        error,
        ClientError::Ssh(255) | ClientError::Timeout | ClientError::RemoteResponse
    ) || matches!(error, ClientError::Io(e) if matches!(e.kind(), std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::TimedOut))
        || matches!(error, ClientError::RemoteFault(code, _, _) if matches!(code.as_str(), "SERVICE_UPDATE_BUSY" | "EXECUTION_BUSY" | "CAPACITY_EXCEEDED"))
}

#[cfg(test)]
mod tests;
