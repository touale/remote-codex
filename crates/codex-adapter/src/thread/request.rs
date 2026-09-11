use super::{Thread, attachments, binding, execution_policy, settings};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Read,
    Turn,
    Steer,
    Settings(Option<bool>),
    GoalStart,
    GoalDefine,
    GoalControl,
}

pub struct Operation {
    pub kind: OperationKind,
    method: String,
    params: Value,
}

impl Operation {
    pub fn client_message_id(&self) -> Option<&str> {
        matches!(self.kind, OperationKind::Turn | OperationKind::Steer)
            .then(|| self.params["clientUserMessageId"].as_str())
            .flatten()
    }
}

pub enum Prepared {
    Immediate(Value),
    Operation(Operation),
}

impl Thread {
    pub fn prepare(
        &self,
        method: &str,
        mut params: Value,
        full_access: bool,
        persisted: bool,
        skills: &BTreeMap<String, String>,
    ) -> Result<Prepared, Fault> {
        if method == "initialize" {
            return Ok(Prepared::Immediate(self.codex.initialized.clone()));
        }
        if matches!(method, "thread/list" | "thread/loaded/list") {
            return Ok(Prepared::Immediate(if method == "thread/list" {
                json!({"data":[],"nextCursor":null})
            } else {
                json!({"data":[self.binding.session.id],"nextCursor":null})
            }));
        }
        if (method.starts_with("thread/") || method.starts_with("turn/"))
            && params["threadId"].as_str() != Some(&self.binding.session.id)
        {
            return Err(Fault::new(
                "SESSION_MISMATCH",
                "use remote-codex to select or create another session",
            ));
        }
        if method == "thread/unsubscribe" {
            return Ok(Prepared::Immediate(json!({"status":"unsubscribed"})));
        }
        if matches!(method, "thread/turns/list" | "thread/items/list") && !persisted {
            return Ok(Prepared::Immediate(
                json!({"data":[],"nextCursor":null,"backwardsCursor":null}),
            ));
        }
        let kind = match method {
            "fs/createDirectory" | "fs/writeFile" | "fs/readFile" => {
                attachments::prepare(method, &mut params, Path::new(&self.binding.codex_home))?;
                OperationKind::Read
            }
            "thread/goal/set" => {
                settings::known_fields(
                    &params,
                    &["threadId", "objective", "status", "tokenBudget"],
                )?;
                if params["status"] == "active"
                    || (params["objective"].is_string() && params["status"] != "paused")
                {
                    OperationKind::GoalStart
                } else if params["objective"].is_string() {
                    OperationKind::GoalDefine
                } else {
                    OperationKind::GoalControl
                }
            }
            "thread/goal/clear" => {
                settings::known_fields(&params, &["threadId"])?;
                OperationKind::GoalControl
            }
            "thread/settings/update" | "turn/settings/update" => {
                settings::prepare_thread(method, &params, &self.binding)?;
                OperationKind::Settings(permission_intent(&params))
            }
            "config/batchWrite" | "config/value/write" => {
                settings::prepare_config(method, &mut params, Path::new(&self.binding.codex_home))?;
                OperationKind::Read
            }
            "turn/start" => {
                binding::turn_params(&mut params, &self.binding);
                execution_policy::validate(&params, &self.binding, full_access)?;
                OperationKind::Turn
            }
            "turn/steer" => {
                if params["expectedTurnId"].as_str().is_none_or(str::is_empty) {
                    return Err(Fault::new(
                        "EXPECTED_TURN_REQUIRED",
                        "Select the running turn before sending a follow-up.",
                    ));
                }
                OperationKind::Steer
            }
            "thread/resume"
            | "thread/read"
            | "thread/turns/list"
            | "thread/items/list"
            | "thread/goal/get"
            | "thread/name/set"
            | "turn/interrupt"
            | "model/list"
            | "collaborationMode/list"
            | "account/read"
            | "account/login/start"
            | "account/login/cancel"
            | "account/rateLimits/read"
            | "config/read"
            | "configRequirements/read"
            | "skills/list"
            | "plugin/list"
            | "hooks/list"
            | "mcpServerStatus/list" => OperationKind::Read,
            _ => {
                return Err(Fault::new(
                    "UNSUPPORTED_CODEX_METHOD",
                    "this native operation is not validated for a bound remote workspace",
                ));
            }
        };
        if matches!(kind, OperationKind::Turn | OperationKind::Steer)
            && let Some(inputs) = params["input"].as_array_mut()
        {
            for input in inputs {
                if input["type"] == "skill"
                    && let Some(path) = input["path"].as_str().and_then(|p| skills.get(p))
                {
                    input["path"] = json!(path);
                }
            }
        }
        Ok(Prepared::Operation(Operation {
            kind,
            method: method.into(),
            params,
        }))
    }

    pub async fn execute(&self, operation: Operation) -> Result<Value, Fault> {
        if operation.method == "thread/resume" {
            let current = self
                .codex
                .engine
                .call(
                    "thread/read",
                    json!({"threadId":self.binding.session.id,"includeTurns":false}),
                )
                .await?;
            let mut response = self.bootstrap.clone();
            response["thread"] = current["thread"].clone();
            return Ok(response);
        }
        self.codex
            .engine
            .call(&operation.method, operation.params)
            .await
    }
}

fn permission_intent(params: &Value) -> Option<bool> {
    if let Some(preset) = params["permissions"].as_str() {
        Some(preset == ":danger-full-access")
    } else {
        params
            .pointer("/sandboxPolicy/type")
            .and_then(Value::as_str)
            .map(|p| p == "dangerFullAccess")
    }
}

pub fn response(id: Value, result: Result<Value, Fault>) -> Value {
    match result {
        Ok(value) => json!({"id":id,"result":value}),
        Err(error) => {
            json!({"id":id,"error":{"code":-32000,"message":error.message,"data":{"code":error.code,"outcome_unknown":error.outcome_unknown}}})
        }
    }
}
