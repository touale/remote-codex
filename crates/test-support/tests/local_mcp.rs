use remote_codex_adapter::{engine::Engine, program::Launch};
use remote_codex_test_support::{
    ProbeResult,
    model::{ModelFixture, function, message},
};
use serde_json::{Value, json};
use std::{os::unix::fs::PermissionsExt, time::Duration};

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; temporary history and loopback model only"]
async fn local_mcp_path_works_for_new_and_cold_resumed_sessions() -> ProbeResult<()> {
    let home = tempfile::tempdir()?;
    let bin = home.path().join("user tools");
    std::fs::create_dir(&bin)?;
    let fixture = bin.join("remote-codex-mcp-fixture");
    std::fs::write(&fixture, include_str!("../fixtures/mcp.sh"))?;
    std::fs::set_permissions(&fixture, std::fs::Permissions::from_mode(0o700))?;
    let launch = Launch {
        path: std::env::var("REMOTE_CODEX_TEST_BINARY")?.into(),
        search_path: Some(std::env::join_paths([
            std::path::Path::new("/usr/bin"),
            std::path::Path::new("/bin"),
            &bin,
        ])?),
    };
    let model = ModelFixture::start().await?;
    let config = format!(
        "model=\"gpt-5.4-mini\"\nmodel_provider=\"fixture\"\n[model_providers.fixture]\nname=\"Local MCP contract\"\nbase_url={}\nwire_api=\"responses\"\nrequires_openai_auth=false\n[analytics]\nenabled=false\n",
        serde_json::to_string(&model.base_url)?
    );
    std::fs::write(home.path().join("config.toml"), &config)?;
    // Create actual persisted history before this MCP is configured.
    let (engine, _) = Engine::local(&launch, home.path()).await?;
    let original = async {
        let thread = engine
            .call("thread/start", json!({"cwd":home.path()}))
            .await?;
        let id = thread["thread"]["id"]
            .as_str()
            .ok_or("thread missing")?
            .to_owned();
        turn(&engine, &id).await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(id)
    }
    .await;
    engine.shutdown().await;
    let original = original?;
    std::fs::write(
        home.path().join("config.toml"),
        format!(
            "{config}[mcp_servers.fixture]\ncommand=\"remote-codex-mcp-fixture\"\nrequired=true\ncwd={}\n[mcp_servers.fixture.env]\nPROBE_MARKER=\"MCP_PATH_READY\"\n",
            serde_json::to_string(home.path())?
        ),
    )?;
    let (engine, _) = Engine::local(&launch, home.path()).await?;
    let result: ProbeResult<()> = async {
        for (method, mut params) in [
            (
                "thread/resume",
                json!({"threadId":original,"excludeTurns":true}),
            ),
            ("thread/start", json!({"cwd":home.path()})),
        ] {
            params["config"] = json!({"mcp_servers.fixture": {
                "command":"remote-codex-mcp-fixture", "cwd":home.path(),
                "required":true, "environment_id":"local", "env":{"PROBE_MARKER":"MCP_PATH_READY"}
            }});
            let thread = engine.call(method, params).await?;
            let id = thread["thread"]["id"].as_str().ok_or("thread missing")?;
            probe_identity(&engine, &model, id, "MCP_PATH_READY").await?;
        }
        Ok(())
    }
    .await;
    engine.shutdown().await;
    result?;

    // Rebuilding the native backend must use current overrides, including removals,
    // rather than reviving the MCP settings persisted with this thread.
    for (enabled, marker) in [
        (Some(true), "UPDATED"),
        (None, "REMOVED"),
        (Some(true), "RESTORED"),
        (Some(false), "DISABLED"),
    ] {
        let settings = json!({"command":"remote-codex-mcp-fixture","cwd":home.path(),"required":true,
            "enabled":enabled.unwrap_or(false),"env":{"PROBE_MARKER":marker}});
        let mut current = config.clone();
        let mut overrides = json!({});
        if enabled.is_some() {
            current.push_str(&toml::to_string(
                &json!({"mcp_servers":{"fixture":settings}}),
            )?);
            overrides["mcp_servers.fixture"] = settings;
            overrides["mcp_servers.fixture"]["environment_id"] = json!("local");
        }
        std::fs::write(home.path().join("config.toml"), current)?;
        let (engine, _) = Engine::local(&launch, home.path()).await?;
        let result = async {
            engine
                .call(
                    "thread/resume",
                    json!({"threadId":original,"excludeTurns":true,"config":overrides}),
                )
                .await?;
            let status = engine
                .call("mcpServerStatus/list", json!({"threadId":original}))
                .await?;
            let available = status["data"]
                .as_array()
                .ok_or("MCP status missing")?
                .iter()
                .any(|server| {
                    server["name"] == "fixture"
                        && server["tools"]
                            .as_object()
                            .is_some_and(|tools| !tools.is_empty())
                });
            assert_eq!(available, enabled == Some(true), "{marker}: {status}");
            if available {
                probe_identity(&engine, &model, &original, marker).await?;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }
        .await;
        engine.shutdown().await;
        result?;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; synthetic MCP tool and loopback model only"]
async fn permission_presets_control_mcp_write_approval_in_both_directions() -> ProbeResult<()> {
    use remote_codex_client::application::SessionSettings;
    let home = tempfile::tempdir()?;
    let fixture = home.path().join("mcp.sh");
    std::fs::write(&fixture, include_str!("../fixtures/mcp.sh"))?;
    let model = ModelFixture::start().await?;
    std::fs::write(
        home.path().join("config.toml"),
        format!(
            "model=\"gpt-5.4-mini\"\nmodel_provider=\"fixture\"\n[model_providers.fixture]\nname=\"MCP approval contract\"\nbase_url={}\nwire_api=\"responses\"\nrequires_openai_auth=false\n[analytics]\nenabled=false\n[mcp_servers.fixture]\ncommand=\"/bin/sh\"\nargs=[{},\"--write-tool\"]\ncwd={}\nrequired=true\n",
            serde_json::to_string(&model.base_url)?,
            serde_json::to_string(&fixture)?,
            serde_json::to_string(home.path())?
        ),
    )?;
    let (engine, _) = Engine::local(
        &Launch::new(std::env::var("REMOTE_CODEX_TEST_BINARY")?),
        home.path(),
    )
    .await?;
    let result = async {
        let thread = engine.call("thread/start", json!({"cwd":home.path(),"sandbox":"danger-full-access","approvalPolicy":"on-request"})).await?;
        let id = thread["thread"]["id"].as_str().ok_or("thread missing")?;
        for (preset, approval, sandbox) in [
            (None, "on-request", "dangerFullAccess"),
            (Some("full_access"), "never", "dangerFullAccess"),
            (Some("workspace"), "on-request", "workspaceWrite"),
        ] {
            if let Some(preset) = preset {
                let settings: SessionSettings = serde_json::from_value(json!({"permissions":preset,"reviewer":"user"}))?;
                let params = remote_codex_adapter::events::settings(id, settings);
                let mut notifications = engine.subscribe();
                engine.call("thread/settings/update", params).await?;
                tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        let event = notifications.recv().await?;
                        if event["method"] == "thread/settings/updated" {
                            let settings = remote_codex_adapter::desktop::settings(&event["params"]["threadSettings"]);
                            assert_eq!(settings.approval_policy, approval);
                            assert_eq!(settings.full_access, sandbox == "dangerFullAccess");
                            return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
                        }
                    }
                }).await??;
            }
            let call = function("write_file", json!({}), Some("mcp__fixture"));
            let call_id = call["call_id"].clone();
            model.script([
                json!({"type":"tool_search_call","id":"fixture_search","call_id":"fixture_search","execution":"client","arguments":{"query":"fixture write_file","limit":1},"status":"completed"}),
                call, message("MCP approval probe finished."),
            ])?;
            assert_eq!(turn(&engine, id).await?, usize::from(approval != "never"));
            let requests = model.requests()?;
            let inputs = requests.last().and_then(|r| r["input"].as_array()).ok_or("model inputs missing")?;
            let output = inputs.iter().find(|item| item["type"] == "function_call_output" && item["call_id"] == call_id).ok_or("MCP output missing")?.to_string();
            assert_eq!(output.contains("MARKER=UNSET"), approval == "never");
            // Restoring this loaded thread must preserve its actual approval policy.
            let restored = engine.call("thread/resume", json!({"threadId":id,"excludeTurns":true})).await?;
            assert_eq!(restored["approvalPolicy"], approval);
            assert_eq!(restored["sandbox"]["type"], sandbox);
        }
        Ok(())
    }.await;
    engine.shutdown().await;
    result
}

async fn probe_identity(
    engine: &Engine,
    model: &ModelFixture,
    id: &str,
    marker: &str,
) -> ProbeResult<()> {
    let call = function("probe_identity", json!({}), Some("mcp__fixture"));
    let call_id = call["call_id"].clone();
    model.script([
        json!({
            "type":"tool_search_call", "id":"fixture_search", "call_id":"fixture_search",
            "execution":"client", "arguments":{"query":"fixture probe_identity","limit":1},
            "status":"completed"
        }),
        call,
        message("MCP fixture finished."),
    ])?;
    turn(engine, id).await?;
    let requests = model.requests()?;
    let inputs = requests
        .last()
        .and_then(|r| r["input"].as_array())
        .ok_or("model inputs missing")?;
    let output = inputs
        .iter()
        .find(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
        .ok_or("MCP output missing")?;
    assert!(
        output.to_string().contains(&format!("MARKER={marker}")),
        "MCP marker missing: {marker}"
    );
    Ok(())
}

async fn turn(engine: &Engine, id: &str) -> ProbeResult<usize> {
    let mut events = engine.subscribe();
    engine
        .call(
            "turn/start",
            json!({"threadId":id,"input":[{"type":"text","text":"Read the fixture identity."}]}),
        )
        .await?;
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut approvals = 0;
        loop {
            let event: Value = events.recv().await?;
            if event["method"] == "mcpServer/elicitation/request" && event.get("id").is_some() {
                approvals += 1;
                engine
                    .send(json!({"id":event["id"],"result":{"action":"decline","content":null}}))?;
            }
            if event["method"] == "turn/completed" {
                assert_eq!(event["params"]["turn"]["status"], "completed");
                return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(approvals);
            }
        }
    })
    .await?
}
