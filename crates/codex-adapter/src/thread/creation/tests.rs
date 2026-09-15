use super::*;

fn bound() -> Result<SessionBinding, serde_json::Error> {
    serde_json::from_value(json!({
        "server_id":"dev", "remote_identity":"installation", "environment_id":"rc_dev",
        "codex_home":"/local/.codex", "codex_version":"test", "execution_mode":"sandboxed",
        "revision":1, "session":{"id":"source", "title":"test", "cwd":"/remote/project",
        "created_at":1, "updated_at":1, "archived":false, "state":"idle"}
    }))
}

#[test]
fn creation_preserves_native_selection_without_accepting_execution_overrides()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bound()?;
    let request = json!({"threadId":"source", "beforeTurnId":"selected", "excludeTurns":true,
        "model":"chosen-model", "serviceTier":null, "sandbox":"workspace-write", "runtimeWorkspaceRoots":[],
        "config":{"model_reasoning_effort":"high", "shell_environment_policy":{"inherit":"all"},
        "mcp_servers.attacker.command":"unsafe"}, "developerInstructions":"untrusted override"});
    let creation = Creation::prepare("thread/fork", &request, &bound, false)?;
    let mut params = binding::start_params(
        &bound.environment_id,
        &bound.session.cwd,
        &bound.execution_mode,
    );
    params["developerInstructions"] = json!("verified resources");
    params["config"]["mcp_servers.verified.command"] = json!("verified");
    params["dynamicTools"] = json!([]);
    assert_eq!(creation.apply(&mut params), "thread/fork");
    assert_eq!(params["threadId"], "source");
    assert_eq!(params["beforeTurnId"], "selected");
    assert_eq!(params["model"], "chosen-model");
    assert!(params.get("serviceTier").is_some_and(Value::is_null));
    assert_eq!(params["config"]["model_reasoning_effort"], "high");
    assert_eq!(params["config"]["mcp_servers.verified.command"], "verified");
    assert_eq!(params["developerInstructions"], "verified resources");
    assert!(params["config"].get("shell_environment_policy").is_none());
    assert!(
        params["config"]
            .get("mcp_servers.attacker.command")
            .is_none()
    );
    assert!(params.get("environments").is_none());
    assert!(params.get("dynamicTools").is_none());
    Ok(())
}

#[test]
fn creation_cannot_change_source_workspace_or_raise_unconfirmed_permissions()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = bound()?;
    for (key, value) in [
        ("threadId", json!("foreign")),
        ("path", json!("/local/other.jsonl")),
        ("cwd", json!("/local/project")),
        ("environments", json!([{"environmentId":"local"}])),
        ("runtimeWorkspaceRoots", json!(["/"])),
        ("sandbox", json!("danger-full-access")),
        ("permissions", json!(":danger-full-access")),
        ("ephemeral", json!(true)),
    ] {
        let mut request = json!({"threadId":"source", "beforeTurnId":"selected"});
        request[key] = value;
        assert!(
            Creation::prepare("thread/fork", &request, &bound, false).is_err(),
            "{key}"
        );
    }
    let request = json!({"permissions":":danger-full-access"});
    let creation = Creation::prepare("thread/start", &request, &bound, true)?;
    let mut params = binding::start_params(
        &bound.environment_id,
        &bound.session.cwd,
        &bound.execution_mode,
    );
    assert_eq!(creation.apply(&mut params), "thread/start");
    assert_eq!(params["permissions"], ":danger-full-access");
    assert!(params.get("sandbox").is_none());
    assert_eq!(params["environments"][0]["environmentId"], "rc_dev");
    Ok(())
}
