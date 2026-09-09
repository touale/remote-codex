use serde_json::Value;

#[derive(Default)]
pub(super) struct Intent {
    pub active: Option<String>,
    pub interrupted: Option<String>,
    pub episode: Option<String>,
    pub settings: Value,
    pub evidence: Value,
    cancelled: Option<String>,
}

impl Intent {
    pub(super) fn disconnect(&mut self) {
        if self.episode.is_none() {
            self.episode = Some(uuid::Uuid::new_v4().to_string());
            self.interrupted = self
                .active
                .clone()
                .filter(|turn| self.cancelled.as_ref() != Some(turn));
        }
    }
    pub(super) fn cancel(&mut self, turn: &str) {
        self.cancelled = Some(turn.into());
        if self.interrupted.as_deref() == Some(turn) {
            self.interrupted = None;
        }
        if self.active.as_deref() == Some(turn) {
            self.active = None;
        }
    }

    pub(super) fn is_cancelled(&self, turn: &str) -> bool {
        self.cancelled.as_deref() == Some(turn)
    }

    pub(super) fn continuation(&mut self, rebuilt: bool) -> Option<(String, String, Value)> {
        if !rebuilt && self.active.is_some() {
            return None;
        }
        let episode = self.episode.take();
        let interrupted = self.interrupted.take();
        if rebuilt {
            self.active = None;
        }
        match (episode, interrupted) {
            (Some(episode), Some(turn)) => Some((episode, turn, self.evidence.clone())),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Intent;
    #[test]
    fn recovery_consumes_intent_once_and_never_restarts_a_cancelled_turn() {
        let mut intent = Intent {
            active: Some("original".into()),
            ..Default::default()
        };
        intent.disconnect();
        assert!(intent.continuation(false).is_none());
        assert!(intent.continuation(true).is_some());
        assert!(intent.continuation(true).is_none());
        intent.active = Some("cancelled".into());
        intent.disconnect();
        intent.cancel("cancelled");
        assert!(intent.continuation(true).is_none());
        // A late native started notification cannot resurrect cancellation.
        intent.active = Some("cancelled".into());
        intent.disconnect();
        assert!(intent.continuation(true).is_none());
    }
}
