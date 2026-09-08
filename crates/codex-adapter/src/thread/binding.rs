use remote_codex_core::session::{Session, SessionBinding};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
pub(crate) fn start_params(environment: &str, cwd: &str, mode: &str) -> Value {
    json!({"cwd":cwd,"environments":[{"environmentId":environment,"cwd":cwd}],
        "approvalPolicy":"on-request","sandbox":if mode=="unrestricted" {"danger-full-access"} else {"workspace-write"},
        "config":{"features.multi_agent":false},"experimentalRawEvents":false})
}

pub(crate) fn turn_params(params: &mut Value, binding: &SessionBinding) {
    params["cwd"] = json!(binding.session.cwd);
    params["environments"] =
        json!([{"environmentId":binding.environment_id,"cwd":binding.session.cwd}]);
    params["runtimeWorkspaceRoots"] = json!([binding.session.cwd]);
}

pub(crate) fn session(thread: &Value, cwd: &str) -> Result<Session, Fault> {
    Ok(Session {
        id: thread["id"]
            .as_str()
            .ok_or(Fault::new(
                "INVALID_NATIVE_THREAD",
                "native thread identity is missing",
            ))?
            .into(),
        title: thread["name"]
            .as_str()
            .or_else(|| thread["preview"].as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("New session")
            .into(),
        cwd: cwd.into(),
        created_at: thread["createdAt"].as_i64().unwrap_or(0),
        updated_at: thread["updatedAt"].as_i64().unwrap_or(0),
        archived: thread["archived"].as_bool().unwrap_or(false),
        state: thread
            .pointer("/status/type")
            .and_then(Value::as_str)
            .unwrap_or("idle")
            .into(),
    })
}
