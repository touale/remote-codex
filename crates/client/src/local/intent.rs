use remote_codex_core::session::SessionEvent;
use serde_json::Value;

#[derive(Default)]
pub(super) struct Intent {
    pub active: Option<String>,
    pub status: remote_codex_core::status::SessionStatus,
    pub current_turn: Option<remote_codex_core::status::TurnState>,
    pub limits_revision: u64,
    pub completed: Option<String>,
    pub goal: Option<remote_codex_core::goals::Goal>,
    pub goal_revision: u64,
    pub plan: Option<remote_codex_core::goals::Plan>,
    pub resume_goal: Option<String>,
    pub interrupted: Option<String>,
    pub episode: Option<String>,
    pub settings: Value,
    cancelled: Option<String>,
}

impl Intent {
    pub(super) fn observe(&mut self, event: &SessionEvent) {
        match event {
            SessionEvent::GoalChanged { goal } => {
                self.goal = goal.clone();
                self.goal_revision = self.goal_revision.wrapping_add(1);
            }
            SessionEvent::PlanChanged { plan } => self.plan = Some(plan.clone()),
            SessionEvent::ActivityChanged {
                activity,
                active_flags,
            } => {
                self.status.activity = activity.clone();
                self.status.active_flags = active_flags.clone();
            }
            SessionEvent::UsageChanged { usage } => self.status.usage = Some(usage.clone()),
            SessionEvent::RateLimitsChanged { limits } => {
                self.status.limits = limits.clone();
                self.status.limits_error = None;
                self.status.limits_updated_at = Some(super::status::now());
                self.limits_revision = self.limits_revision.wrapping_add(1);
            }
            SessionEvent::TurnStarted { id, timing } => {
                self.current_turn = Some(remote_codex_core::status::TurnState {
                    id: id.clone(),
                    status: "inProgress".into(),
                    timing: timing.clone(),
                });
            }
            SessionEvent::TurnCompleted {
                id,
                timing,
                outcome,
            } => {
                let mut timing = timing.clone();
                if let Some(previous) = &self.current_turn
                    && previous.id == *id
                {
                    timing.started_at = timing.started_at.or(previous.timing.started_at);
                }
                if self
                    .current_turn
                    .as_ref()
                    .is_some_and(|current| current.id != *id)
                {
                    return;
                }
                self.current_turn = Some(remote_codex_core::status::TurnState {
                    id: id.clone(),
                    timing,
                    status: match outcome {
                        remote_codex_core::session::TurnOutcome::Completed => "completed",
                        remote_codex_core::session::TurnOutcome::Interrupted => "interrupted",
                        remote_codex_core::session::TurnOutcome::Failed { .. } => "failed",
                    }
                    .into(),
                });
            }
            _ => {}
        }
    }
    pub(super) fn with_status(status: remote_codex_core::status::SessionStatus) -> Self {
        Self {
            status,
            ..Default::default()
        }
    }
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

    pub(super) fn continuation(&mut self, rebuilt: bool) -> Option<(String, String)> {
        if !rebuilt && self.active.is_some() {
            return None;
        }
        let episode = self.episode.take();
        let interrupted = self.interrupted.take();
        if rebuilt {
            self.active = None;
        }
        match (episode, interrupted) {
            (Some(episode), Some(turn)) => Some((episode, turn)),
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
