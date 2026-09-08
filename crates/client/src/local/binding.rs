use super::*;
use remote_codex_protocol::Session;

pub(super) fn validate(
    binding: &SessionBinding,
    remote: &Remote,
    home: &Path,
    mode: &str,
) -> Result<()> {
    if binding.server_id != remote.server.id || binding.remote_identity != remote.identity.identity
    {
        return Err(ClientError::Argument(
            "session belongs to another execution environment",
        ));
    }
    if Path::new(&binding.codex_home) != home {
        return Err(ClientError::Argument(
            "session belongs to a different local CODEX_HOME",
        ));
    }
    if binding.execution_mode != mode {
        return Err(ClientError::Argument(
            "environment execution mode changed; create a new session to use the new permissions",
        ));
    }
    if binding.codex_version != crate::runtime::CANDIDATE_VERSION {
        return Err(ClientError::Unsupported(
            "session Codex version has no validated migration",
        ));
    }
    Ok(())
}

pub(super) fn start_params(environment: &str, cwd: &str, mode: &str) -> Value {
    json!({"cwd":cwd,"environments":[{"environmentId":environment,"cwd":cwd}],
        "approvalPolicy":"on-request","sandbox":if mode=="unrestricted" {"danger-full-access"} else {"workspace-write"},
        "config":{"features.multi_agent":false},"experimentalRawEvents":false})
}

pub(super) fn turn_params(params: &mut Value, binding: &SessionBinding) {
    params["cwd"] = json!(binding.session.cwd);
    params["environments"] =
        json!([{"environmentId":binding.environment_id,"cwd":binding.session.cwd}]);
    params["runtimeWorkspaceRoots"] = json!([binding.session.cwd]);
}

pub(super) fn session(thread: &Value, cwd: &str) -> Result<Session> {
    Ok(Session {
        id: thread["id"]
            .as_str()
            .ok_or(ClientError::RemoteResponse)?
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
