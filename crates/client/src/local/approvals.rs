use remote_codex_protocol::{CommandApproval, Fault, SessionBinding};
use serde_json::{Value, json};
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
        event: &mut Value,
        binding: &SessionBinding,
        channel: &str,
    ) -> Result<(), Fault> {
        let mut state = self.0.lock().map_err(|_| invalid())?;
        let method = event["method"].as_str().unwrap_or_default();
        if matches!(method, "item/completed" | "turn/completed") {
            let item = event.pointer("/params/item/id").and_then(Value::as_str);
            let turn = event.pointer("/params/turn/id").and_then(Value::as_str);
            state
                .accepted
                .retain(|(_, grant)| item != Some(&grant.item) && turn != Some(&grant.turn));
            state
                .pending
                .retain(|_, grant| item != Some(&grant.item) && turn != Some(&grant.turn));
        }
        if method != "item/commandExecution/requestApproval"
            || binding.execution_mode != "sandboxed"
        {
            return Ok(());
        }
        let params = &event["params"];
        if params["kind"]
            .as_str()
            .is_some_and(|kind| kind != "command")
        {
            return Ok(());
        }
        if params["threadId"].as_str() != Some(&binding.session.id)
            || params["environmentId"].as_str() != Some(&binding.environment_id)
        {
            return Err(invalid());
        }
        let command = params["command"].as_str().ok_or_else(invalid)?;
        let argv = shlex::split(command)
            .filter(|argv| !argv.is_empty())
            .ok_or_else(invalid)?;
        let cwd = url::Url::from_file_path(params["cwd"].as_str().ok_or_else(invalid)?)
            .map_err(|_| invalid())?
            .to_string();
        let approval = CommandApproval {
            id: uuid::Uuid::new_v4().to_string(),
            channel: channel.into(),
            thread: binding.session.id.clone(),
            turn: params["turnId"].as_str().ok_or_else(invalid)?.into(),
            item: params["itemId"].as_str().ok_or_else(invalid)?.into(),
            argv,
            cwd,
        };
        if state.pending.len() + state.accepted.len() >= 64 {
            return Err(invalid());
        }
        let id = event.get("id").ok_or_else(invalid)?.to_string();
        state.pending.insert(id, approval);
        // A permanent native rule cannot be represented by a one-use grant.
        event["params"]["availableDecisions"] = json!(["accept", "cancel"]);
        event["params"]["proposedExecpolicyAmendment"] = Value::Null;
        Ok(())
    }

    pub(crate) fn respond(&self, response: &Value) -> Result<(), Fault> {
        let mut state = self.0.lock().map_err(|_| invalid())?;
        let Some(grant) = state.pending.remove(&response["id"].to_string()) else {
            return Ok(());
        };
        match response.pointer("/result/decision").and_then(Value::as_str) {
            Some("accept") => state.accepted.push((Instant::now(), grant)),
            Some("cancel" | "decline") | None => {}
            _ => {
                return Err(Fault::new(
                    "UNSUPPORTED_APPROVAL_SCOPE",
                    "this execution environment supports approving this command once",
                ));
            }
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
