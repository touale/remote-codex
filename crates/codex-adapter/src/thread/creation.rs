use super::{binding, execution_policy};
use remote_codex_core::session::SessionBinding;
use remote_codex_protocol::Fault;
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

/// A frontend creation request restricted to the workspace it already controls.
pub struct Creation {
    method: &'static str,
    params: Value,
    pub allow_full_access: bool,
}

impl Creation {
    pub fn prepare(
        method: &str,
        params: &Value,
        bound: &SessionBinding,
        full_access: bool,
    ) -> Result<Self, Fault> {
        let method = match method {
            "thread/start" => "thread/start",
            "thread/fork" if params["threadId"] == bound.session.id => "thread/fork",
            _ => {
                return Err(Fault::new(
                    "SESSION_MISMATCH",
                    "select a session in this workspace",
                ));
            }
        };
        if params["path"].as_str().is_some_and(|path| !path.is_empty())
            || params["ephemeral"] == true
        {
            return Err(Fault::new(
                "UNSUPPORTED_CODEX_METHOD",
                "this workspace requires a bound, persistent thread",
            ));
        }
        for (key, expected) in [
            ("cwd", json!(bound.session.cwd)),
            ("runtimeWorkspaceRoots", json!([bound.session.cwd])),
            (
                "environments",
                json!([{"environmentId":bound.environment_id,"cwd":bound.session.cwd}]),
            ),
        ] {
            if key == "runtimeWorkspaceRoots" && params[key] == json!([]) {
                continue;
            }
            if params
                .get(key)
                .is_some_and(|value| !value.is_null() && value != &expected)
            {
                return Err(Fault::new(
                    "WORKSPACE_SETTINGS_LOCKED",
                    "new threads must preserve this execution workspace",
                ));
            }
        }
        let mut policy = json!({"cwd":bound.session.cwd});
        for key in ["permissions", "approvalsReviewer"] {
            if let Some(value) = params.get(key).filter(|v| !v.is_null()) {
                policy[key] = value.clone();
            }
        }
        if let Some(sandbox) = params["sandbox"].as_str() {
            policy["sandboxPolicy"] = json!({"type":match sandbox {
                "read-only" => "readOnly",
                "workspace-write" => "workspaceWrite",
                "danger-full-access" => "dangerFullAccess",
                _ => return Err(Fault::new("INVALID_NATIVE_SETTINGS", "unsupported sandbox mode")),
            }});
        }
        execution_policy::validate(&policy, bound, full_access)?;
        let mut safe = json!({"runtimeWorkspaceRoots":[bound.session.cwd]});
        for key in [
            "model",
            "modelProvider",
            "serviceTier",
            "approvalPolicy",
            "approvalsReviewer",
            "sandbox",
            "permissions",
            "threadSource",
            "historyMode",
        ] {
            if let Some(value) = params
                .get(key)
                .filter(|v| !v.is_null() || key == "serviceTier")
            {
                safe[key] = value.clone();
            }
        }
        if method == "thread/fork" {
            safe["threadId"] = json!(bound.session.id);
            safe["excludeTurns"] = json!(true);
            for key in ["beforeTurnId", "lastTurnId", "deferGoalContinuation"] {
                if let Some(value) = params.get(key).filter(|v| !v.is_null()) {
                    safe[key] = value.clone();
                }
            }
        }
        // Execution and extension configuration comes from the trusted recipe.
        // The TUI may carry local session flags that do not apply to this server.
        for key in [
            "model_reasoning_effort",
            "model_reasoning_summary",
            "model_verbosity",
            "personality",
            "web_search",
        ] {
            if let Some(value) = params["config"].get(key) {
                safe["config"][key] = value.clone();
            }
        }
        Ok(Self {
            method,
            params: safe,
            allow_full_access: full_access || binding::full_access(&bound.execution_mode),
        })
    }

    pub fn is_fork(&self) -> bool {
        self.method == "thread/fork"
    }

    pub(super) fn apply(&self, params: &mut Value) -> &'static str {
        if let Some(object) = params.as_object_mut() {
            if self.is_fork() {
                for key in ["environments", "dynamicTools"] {
                    object.remove(key);
                }
            }
            if self.params.get("permissions").is_some() {
                object.remove("sandbox");
            }
        }
        if let Some(overrides) = self.params.as_object() {
            for (key, value) in overrides {
                if key == "config" {
                    if let Some(config) = value.as_object() {
                        for (key, value) in config {
                            params["config"][key] = value.clone();
                        }
                    }
                } else {
                    params[key] = value.clone();
                }
            }
        }
        self.method
    }
}
