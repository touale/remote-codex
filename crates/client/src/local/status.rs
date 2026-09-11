use super::LocalRuntime;
use crate::{ClientError, Result};
use remote_codex_core::desktop::SessionSnapshot;

pub(super) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

impl LocalRuntime {
    /// Reading local Codex status neither connects SSH nor starts an inference turn.
    pub(crate) async fn status(&self) -> Result<SessionSnapshot> {
        if self.closed.borrow().is_some() || *self.lease.revoked.borrow() {
            return self.snapshot();
        }
        let generation = self.current()?;
        let revision = self
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .limits_revision;
        let limits = generation.native.rate_limits().await;
        if self.current()?.bridge.channel == generation.bridge.channel {
            let mut intent = self
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            if intent.limits_revision == revision {
                match limits {
                    Ok(limits) => {
                        intent.status.limits = limits;
                        intent.status.limits_error = None;
                        intent.status.limits_updated_at = Some(now());
                    }
                    Err(error) => intent.status.limits_error = Some(error.message),
                }
            }
        }
        self.snapshot()
    }
}
