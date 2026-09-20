use super::*;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn binding() -> Result<SessionBinding, serde_json::Error> {
    serde_json::from_value(json!({
        "server_id":"dev", "remote_identity":"installation", "environment_id":"rc_dev",
        "codex_home":"/local/.codex", "codex_version":"0.153.4", "execution_mode":"sandboxed",
        "revision":1, "session":{"id":"thread", "title":"test", "cwd":"/remote/project",
        "created_at":1, "updated_at":1, "archived":false, "state":"idle"}
    }))
}

fn bootstrap() -> Value {
    json!({"approvalPolicy":"on-request", "approvalsReviewer":"user",
        "sandbox":{"type":"workspaceWrite", "writableRoots":["/remote/project"],
        "networkAccess":false, "excludeTmpdirEnvVar":true, "excludeSlashTmp":true}})
}

#[test]
fn recovery_preserves_read_only_policy_and_rejects_unconfirmed_full_access() -> TestResult {
    let read_only = json!({"type":"readOnly", "networkAccess":false});
    let snapshot = json!({"model":"gpt-6-astra", "effort":"low", "serviceTier":null,
        "sandboxPolicy":read_only, "approvalPolicy":"on-request", "approvalsReviewer":"user"});
    let restored = restore(&snapshot, &binding()?, false)?;
    assert_eq!(restored["sandboxPolicy"], read_only);
    assert!(restored.get("permissions").is_none());
    assert!(restored.get("serviceTier").is_some_and(Value::is_null));
    let full = json!({"sandboxPolicy":{"type":"dangerFullAccess"}});
    assert!(restore(&full, &binding()?, false).is_err());
    assert_eq!(
        restore(&full, &binding()?, true)?["permissions"],
        ":danger-full-access"
    );
    Ok(())
}

#[test]
fn model_changes_preserve_native_fields_and_null_service_tier() -> TestResult {
    let params = json!({"threadId":"thread", "model":"gpt-6-astra", "effort":"xhigh",
        "serviceTier":null, "collaborationMode":{"mode":"default", "settings":{
        "model":"gpt-6-astra", "reasoning_effort":"xhigh", "developer_instructions":null}}});
    prepare_thread("thread/settings/update", &params, &binding()?)?;
    assert!(params.get("serviceTier").is_some_and(Value::is_null));
    assert_eq!(
        params["collaborationMode"]["settings"]["reasoning_effort"],
        "xhigh"
    );
    Ok(())
}

#[test]
fn thread_updates_cannot_rebind_or_expand_execution_authority() -> TestResult {
    for (key, value) in [
        ("cwd", json!("/local/project")),
        ("permissions", json!("full-access")),
        ("environments", json!([{"environmentId":"local"}])),
        ("threadId", json!("another-thread")),
    ] {
        let mut params = json!({"threadId":"thread", "model":"gpt-6-astra"});
        params[key] = value;
        assert!(
            prepare_thread("thread/settings/update", &params, &binding()?).is_err(),
            "{key}"
        );
    }
    let mut current = bootstrap();
    current["threadId"] = json!("thread");
    current["cwd"] = json!("/remote/project");
    current["sandboxPolicy"] = current
        .as_object_mut()
        .ok_or("expected object")?
        .remove("sandbox")
        .ok_or("expected sandbox")?;
    prepare_thread("thread/settings/update", &current, &binding()?)?;
    Ok(())
}

#[test]
fn running_turn_settings_allow_model_preferences_but_not_thread_only_fields() -> TestResult {
    let mut params =
        json!({"threadId":"thread", "turnId":"turn", "model":"gpt-6-astra", "effort":"high"});
    prepare_thread("turn/settings/update", &params, &binding()?)?;
    params["collaborationMode"] = json!({"mode":"plan"});
    assert!(prepare_thread("turn/settings/update", &params, &binding()?).is_err());
    Ok(())
}

#[test]
fn defaults_target_local_codex_and_preserve_optimistic_version() -> TestResult {
    let mut params = json!({"edits":[
        {"keyPath":"model", "value":"gpt-6-astra", "mergeStrategy":"replace"},
        {"keyPath":"model_reasoning_effort", "value":"xhigh", "mergeStrategy":"replace"},
        {"keyPath":"profiles.work.service_tier", "value":null, "mergeStrategy":"upsert"}
    ], "expectedVersion":"original-version", "filePath":null, "reloadUserConfig":true});
    let edits = params["edits"].clone();
    prepare_config("config/batchWrite", &mut params, &binding()?)?;
    assert_eq!(params["edits"], edits);
    assert_eq!(params["filePath"], "/local/.codex/config.toml");
    assert_eq!(params["expectedVersion"], "original-version");
    assert_eq!(params["reloadUserConfig"], false);
    Ok(())
}

#[test]
fn config_batches_reject_unsafe_edits_before_any_forwarding() -> TestResult {
    for key in [
        "sandbox_mode",
        "mcp_servers.local.command",
        "environments",
        "profiles.work",
        "profiles.work.sandbox_mode",
        "model.extra",
    ] {
        let mut params = json!({"edits":[
            {"keyPath":"model", "value":"gpt-6-astra", "mergeStrategy":"replace"},
            {"keyPath":key, "value":"unsafe", "mergeStrategy":"replace"}
        ]});
        let before = params.clone();
        assert!(
            prepare_config("config/batchWrite", &mut params, &binding()?).is_err(),
            "{key}"
        );
        assert_eq!(params, before);
    }
    Ok(())
}

#[test]
fn single_writes_validate_file_and_value_type() -> TestResult {
    let valid = json!({"keyPath":"model", "value":"gpt-6-astra", "mergeStrategy":"replace"});
    let mut params = valid.clone();
    prepare_config("config/value/write", &mut params, &binding()?)?;
    for (key, value) in [
        ("filePath", json!("/remote/project/.codex/config.toml")),
        ("value", json!({"sandbox_mode":"danger-full-access"})),
        ("mergeStrategy", json!("unknown")),
        ("expectedVersion", json!(false)),
    ] {
        let mut params = valid.clone();
        params[key] = value;
        assert!(prepare_config("config/value/write", &mut params, &binding()?).is_err());
    }
    Ok(())
}

#[test]
fn native_approve_for_me_preset_is_allowed_without_rebinding() -> TestResult {
    let params = json!({"threadId":"thread", "permissions":":workspace", "sandboxPolicy":null,
        "approvalPolicy":"on-request", "approvalsReviewer":"auto_review"});
    prepare_thread("thread/settings/update", &params, &binding()?)?;
    let mut turn = params.clone();
    turn["cwd"] = json!("/local/incorrect");
    crate::thread::binding::turn_params(&mut turn, &binding()?);
    assert_eq!(turn["cwd"], "/remote/project");
    assert_eq!(turn["permissions"], ":workspace");
    assert_eq!(turn["approvalsReviewer"], "auto_review");
    assert!(turn["sandboxPolicy"].is_null());
    crate::thread::execution_policy::validate(&turn, &binding()?, false)?;
    Ok(())
}

#[test]
fn full_access_selection_requires_separate_execution_authorization() -> TestResult {
    let params = json!({"threadId":"thread", "permissions":":danger-full-access",
        "approvalPolicy":"never", "approvalsReviewer":"user"});
    prepare_thread("thread/settings/update", &params, &binding()?)?;
    assert!(crate::thread::execution_policy::validate(&params, &binding()?, false).is_err());
    crate::thread::execution_policy::validate(&params, &binding()?, true)?;
    let mut trusted = binding()?;
    trusted.execution_mode = "unrestricted".into();
    prepare_thread("thread/settings/update", &params, &trusted)?;
    let mut turn = params.clone();
    crate::thread::binding::turn_params(&mut turn, &trusted);
    assert_eq!(turn["approvalPolicy"], "never");
    Ok(())
}

#[test]
fn sandboxed_ceiling_rejects_extra_roots_network_and_external_sandbox() -> TestResult {
    for sandbox in [
        json!({"type":"workspaceWrite", "networkAccess":true}),
        json!({"type":"workspaceWrite", "writableRoots":["/remote/project/../other"]}),
        json!({"type":"workspaceWrite", "writableRoots":["/remote/other"]}),
        json!({"type":"externalSandbox"}),
    ] {
        assert!(
            crate::thread::execution_policy::validate(
                &json!({"sandboxPolicy":sandbox}),
                &binding()?,
                false
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn reviewer_preferences_can_be_saved_and_invalid_reviewers_are_rejected() -> TestResult {
    for reviewer in [json!("user"), json!("auto_review"), Value::Null] {
        let mut params = json!({"edits":[{"keyPath":"approvals_reviewer", "value":reviewer, "mergeStrategy":"replace"}], "reloadUserConfig":true});
        prepare_config("config/batchWrite", &mut params, &binding()?)?;
        assert_eq!(params["reloadUserConfig"], false);
    }
    let mut params =
        json!({"keyPath":"approvals_reviewer", "value":"anyone", "mergeStrategy":"replace"});
    assert!(prepare_config("config/value/write", &mut params, &binding()?).is_err());
    Ok(())
}

#[test]
fn project_trust_is_limited_to_the_bound_workspace_and_valid_choices() -> TestResult {
    let mut bound = binding()?;
    bound.session.cwd = r#"/remote/a.b/"quoted"\name"#.into();
    let key = r#"projects."/remote/a.b/\"quoted\"\\name".trust_level"#;
    for value in [json!("trusted"), json!("untrusted")] {
        let mut params = json!({"keyPath":key,"value":value,"mergeStrategy":"upsert"});
        prepare_config("config/value/write", &mut params, &bound)?;
        assert_eq!(params["keyPath"], key);
        assert_eq!(params["filePath"], "/local/.codex/config.toml");
    }
    for (key, value) in [
        (key, Value::Null),
        (key, json!("anything")),
        (key, json!({"trust_level":"trusted"})),
        (r#"projects."/remote/other".trust_level"#, json!("trusted")),
        (r#"projects."/remote".trust_level"#, json!("trusted")),
        (
            "projects",
            json!({"/remote/other":{"trust_level":"trusted"}}),
        ),
    ] {
        let mut params = json!({"edits":[
            {"keyPath":"model","value":"gpt-6-astra","mergeStrategy":"replace"},
            {"keyPath":key,"value":value,"mergeStrategy":"upsert"}
        ]});
        let before = params.clone();
        assert!(prepare_config("config/batchWrite", &mut params, &bound).is_err());
        assert_eq!(params, before);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; temporary Codex home, no model turn"]
async fn native_project_trust_survives_restart_without_changing_execution_defaults() -> TestResult {
    use crate::engine::Engine;
    let program = crate::program::Launch::new(std::env::var("REMOTE_CODEX_TEST_BINARY")?);
    let home = tempfile::tempdir()?;
    let mut bound = binding()?;
    bound.codex_home = home.path().to_str().ok_or("invalid test path")?.into();
    std::fs::write(
        home.path().join("config.toml"),
        "sandbox_mode=\"read-only\"\n",
    )?;
    let (engine, _) = Engine::local(&program, home.path()).await?;
    let result = async {
        let mut params = json!({"edits":[{
            "keyPath":"projects.\"/remote/project\".trust_level",
            "value":"trusted","mergeStrategy":"upsert"
        }],"reloadUserConfig":true});
        prepare_config("config/batchWrite", &mut params, &bound)?;
        assert_eq!(params["reloadUserConfig"], false);
        engine.call("config/batchWrite", params).await?;
        Ok::<_, Box<dyn std::error::Error>>(())
    }
    .await;
    engine.shutdown().await;
    result?;
    let (reopened, _) = Engine::local(&program, home.path()).await?;
    let config = reopened
        .call("config/read", json!({"includeLayers":false}))
        .await;
    reopened.shutdown().await;
    let config = config?;
    assert_eq!(
        config["config"]["projects"]["/remote/project"]["trust_level"],
        "trusted"
    );
    assert_eq!(config["config"]["sandbox_mode"], "read-only");
    Ok(())
}
