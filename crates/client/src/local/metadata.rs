use super::{Generation, LocalRuntime};
use crate::Result;
use remote_codex_core::session::SessionEvent;
use std::sync::atomic::Ordering;

impl LocalRuntime {
    pub(super) async fn refresh_summary(&self, generation: &Generation) -> Result<()> {
        if !self.has_history.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut binding = generation.native.binding().clone();
        binding.session = generation.native.summary().await?;
        if self.store.update_session(&binding).await? {
            let _ = self.events.send(SessionEvent::SessionUpdated {
                session: binding.session,
            });
        }
        Ok(())
    }
}
