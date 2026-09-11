use super::{Generation, LocalRuntime};
use crate::Result;
use remote_codex_core::session::SessionEvent;
use std::sync::atomic::Ordering;

impl LocalRuntime {
    pub(super) async fn refresh_summary(&self, generation: &Generation) -> Result<()> {
        let session = generation.native.summary().await?;
        if self.has_history.load(Ordering::Acquire) && self.store.save_summary(&session).await? {
            let _ = self.events.send(SessionEvent::SessionUpdated { session });
        }
        Ok(())
    }
}
