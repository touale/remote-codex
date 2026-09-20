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
    let result = async {
        for (method, params) in [
            ("thread/resume", json!({"threadId":original,"excludeTurns":true})),
            ("thread/start", json!({"cwd":home.path()})),
        ] {
            let thread = engine.call(method, params).await?;
            let id = thread["thread"]["id"].as_str().ok_or("thread missing")?;
            let call = function("probe_identity", json!({}), Some("mcp__fixture"));
            let call_id = call["call_id"].clone();
            model.script([
                json!({"type":"tool_search_call","id":"fixture_search","call_id":"fixture_search","execution":"client","arguments":{"query":"fixture probe_identity","limit":1},"status":"completed"}),
                call,
                message("MCP fixture finished."),
            ])?;
            turn(&engine, id).await?;
            let requests = model.requests()?;
            let inputs = requests.last().and_then(|r| r["input"].as_array()).ok_or("model inputs missing")?;
            let output = inputs.iter().find(|item| item["type"] == "function_call_output" && item["call_id"] == call_id).ok_or("MCP output missing")?;
            assert!(output.to_string().contains("MARKER=MCP_PATH_READY"), "{method}: MCP did not execute locally");
        }
        Ok(())
    }.await;
    engine.shutdown().await;
    result
}

async fn turn(engine: &Engine, id: &str) -> ProbeResult<()> {
    let mut events = engine.subscribe();
    engine
        .call(
            "turn/start",
            json!({"threadId":id,"input":[{"type":"text","text":"Read the fixture identity."}]}),
        )
        .await?;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let event: Value = events.recv().await?;
            if event["method"] == "turn/completed" {
                assert_eq!(event["params"]["turn"]["status"], "completed");
                return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
            }
        }
    })
    .await?
}
