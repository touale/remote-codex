use remote_codex_core::session::SessionBinding;
use remote_codex_protocol::{CommandApproval, Fault};
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[cfg(test)]
#[path = "approvals_tests.rs"]
mod tests;

#[derive(Default)]
pub(crate) struct Approvals(Mutex<State>);

#[derive(Default)]
struct State {
    pending: HashMap<String, CommandApproval>,
    accepted: Vec<(Instant, CommandApproval)>,
}

impl Approvals {
    pub(crate) fn observe(
        &self,
        notice: &remote_codex_adapter::events::Notice,
        binding: &SessionBinding,
        channel: &str,
    ) -> Result<(), Fault> {
        use remote_codex_adapter::events::Notice;
        let mut state = self.0.lock().map_err(|_| invalid())?;
        match notice {
            Notice::Completed { item, turn } => {
                state.accepted.retain(|(_, grant)| {
                    item.as_ref() != Some(&grant.item) && turn.as_ref() != Some(&grant.turn)
                });
                state.pending.retain(|_, grant| {
                    item.as_ref() != Some(&grant.item) && turn.as_ref() != Some(&grant.turn)
                });
            }
            Notice::Approval(approval) => {
                if approval.thread != binding.session.id
                    || approval.environment != binding.environment_id
                {
                    return Err(invalid());
                }
                if state.pending.len() + state.accepted.len() >= 64 {
                    return Err(invalid());
                }
                state.pending.insert(
                    approval.request_id.clone(),
                    CommandApproval {
                        id: uuid::Uuid::new_v4().to_string(),
                        channel: channel.into(),
                        thread: approval.thread.clone(),
                        turn: approval.turn.clone(),
                        item: approval.item.clone(),
                        argv: approval.argv.clone(),
                        cwd: approval.cwd.clone(),
                    },
                );
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn respond(&self, id: &str, accepted: bool) -> Result<(), Fault> {
        let mut state = self.0.lock().map_err(|_| invalid())?;
        if let Some(grant) = state.pending.remove(id)
            && accepted
        {
            state.accepted.push((Instant::now(), grant));
        }
        Ok(())
    }

    pub(crate) fn take(&self, message: &Value) -> Result<Option<CommandApproval>, Fault> {
        let mut state = self.0.lock().map_err(|_| invalid())?;
        state
            .accepted
            .retain(|(when, _)| when.elapsed() < Duration::from_secs(60));
        Ok(state
            .accepted
            .iter()
            .position(|(_, grant)| grant.matches(message))
            .map(|index| state.accepted.remove(index).1))
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut state) = self.0.lock() {
            *state = State::default();
        }
    }
}

fn invalid() -> Fault {
    Fault::new(
        "INVALID_EXECUTION_APPROVAL",
        "cannot bind this approval to a remote command",
    )
}
