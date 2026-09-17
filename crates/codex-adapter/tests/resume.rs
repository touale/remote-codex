//! Exercise paginated resume against native Codex with synthetic, interrupted history.
use remote_codex_adapter::thread::{Codex, Creation, OpenSource, OpenThread, Prepared, Thread};
use remote_codex_core::session::SessionBinding;
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const ID: &str = "01990000-0000-7000-8000-000000000001";

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; isolated history, no model requests"]
async fn resume_and_fork_preserve_paginated_history_and_paused_goal() -> TestResult {
    let program = std::env::var("REMOTE_CODEX_TEST_BINARY")?;
    let home = tempfile::tempdir()?;
    let cwd = home.path().to_str().ok_or("invalid temporary path")?;
    std::fs::write(
        home.path().join("config.toml"),
        "model=\"gpt-6-astra\"\nmodel_provider=\"offline\"\n[model_providers.offline]\nname=\"Offline\"\nbase_url=\"http://127.0.0.1:9/v1\"\nwire_api=\"responses\"\nrequires_openai_auth=false\n[analytics]\nenabled=false\n",
    )?;
    history(home.path())?;
    let codex = Codex::start(Path::new(&program), home.path()).await?;
    let result = async {
        let opened = codex
            .open(OpenThread {
                environment: "local",
                directory: cwd,
                execution_mode: "sandboxed",
                source: OpenSource::Resume(ID),
                mcp: BTreeMap::new(),
                instructions: String::new(),
            })
            .await?;
        let binding = SessionBinding {
            session: opened.session.clone(),
            server_id: "test".into(),
            remote_identity: "test".into(),
            environment_id: "local".into(),
            codex_home: cwd.into(),
            codex_version: "0.153.4".into(),
            execution_mode: "sandboxed".into(),
            revision: 1,
        };
        let mut thread = codex.bind(opened, binding, false)?;
        let sandbox = thread.settings_snapshot()["sandbox"].clone();
        call(
            &thread,
            "thread/settings/update",
            json!({"threadId":ID,
            "model":"gpt-5.4-mini", "effort":"low", "approvalsReviewer":"user"}),
        )
        .await?;
        call(
            &thread,
            "thread/goal/set",
            json!({"threadId":ID,
            "objective":"Keep this synthetic goal paused", "status":"paused", "tokenBudget":40000}),
        )
        .await?;
        let goal = call(&thread, "thread/goal/get", json!({"threadId":ID})).await?;
        let resumed = call(
            &thread,
            "thread/resume",
            json!({"threadId":ID,
            "initialTurnsPage":{"limit":2,"itemsView":"full","sortDirection":"desc"},
            "cwd":"/wrong-workspace", "model":"wrong-model", "sandbox":"danger-full-access",
            "excludeTurns":false}),
        )
        .await?;
        assert_eq!(resumed["cwd"], cwd);
        assert_eq!(resumed["model"], "gpt-5.4-mini");
        assert_eq!(resumed["reasoningEffort"], "low");
        assert_eq!(resumed["approvalsReviewer"], "user");
        assert_eq!(resumed["sandbox"], sandbox);
        assert_eq!(resumed["thread"]["status"]["type"], "idle");
        assert_eq!(resumed["thread"]["turns"], json!([]));
        let page = &resumed["initialTurnsPage"];
        assert_eq!(page["data"].as_array().map(Vec::len), Some(2));
        assert_eq!(page["data"][0]["status"], "interrupted");
        assert_eq!(page["data"][0]["items"], json!([]));
        assert!(page["nextCursor"].is_string());
        let older = call(
            &thread,
            "thread/turns/list",
            json!({"threadId":ID,
            "cursor":page["nextCursor"], "limit":2,"itemsView":"full","sortDirection":"desc"}),
        )
        .await?;
        assert_eq!(older["data"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            call(&thread, "thread/goal/get", json!({"threadId":ID})).await?,
            goal
        );
        // Restoring identical settings produces no native notification. Recovery
        // must verify live state instead of treating that no-op as a failure.
        let mut settings = resumed.clone();
        settings["sandboxPolicy"] = settings["sandbox"].clone();
        settings["effort"] = settings["reasoningEffort"].clone();
        assert!(!thread.restore_settings(&settings, false).await?);
        assert!(!thread.restore_settings(&settings, false).await?);
        assert_eq!(thread.bootstrap()["model"], "gpt-5.4-mini");
        assert_eq!(thread.bootstrap()["reasoningEffort"], "low");
        let mut full = settings.clone();
        full["model"] = json!("gpt-6-astra");
        full["effort"] = json!("medium");
        full["sandboxPolicy"] = json!({"type":"dangerFullAccess"});
        assert!(thread.restore_settings(&full, false).await.is_err());
        assert!(thread.restore_settings(&full, true).await?);
        assert!(thread.restore_settings(&full, true).await?);
        assert_eq!(thread.bootstrap()["model"], "gpt-6-astra");
        assert_eq!(thread.bootstrap()["reasoningEffort"], "medium");
        assert!(!thread.restore_settings(&settings, true).await?);
        assert_eq!(thread.bootstrap()["model"], "gpt-5.4-mini");
        assert_eq!(thread.bootstrap()["sandbox"], sandbox);
        assert_eq!(
            call(&thread, "thread/goal/get", json!({"threadId":ID})).await?,
            goal
        );
        let mut events = thread.take_events()?;
        while let Ok(event) = events.try_recv() {
            assert_ne!(event["method"], "deprecationNotice");
            assert_ne!(event["method"], "turn/started");
        }
        fork_paused_goal(&thread, Path::new(&program), home.path(), &goal).await?;
        Ok(())
    }
    .await;
    codex.shutdown().await;
    result
}

async fn fork_paused_goal(
    source: &Thread,
    program: &Path,
    home: &Path,
    expected: &Value,
) -> TestResult {
    let codex = Codex::start(program, home).await?;
    let result = async {
        let creation = Creation::prepare(
            "thread/fork",
            &json!({
            "threadId":ID, "beforeTurnId":"01990000-0000-7000-8000-000000000003",
            "deferGoalContinuation":true,
                "model":"gpt-5.4-mini", "config":{"model_reasoning_effort":"low"},
            }),
            source.binding(),
            false,
        )?;
        let opened = codex
            .open(OpenThread {
                environment: "local",
                directory: &source.binding().session.cwd,
                execution_mode: "sandboxed",
                source: OpenSource::Frontend(&creation),
                mcp: BTreeMap::new(),
                instructions: String::new(),
            })
            .await?;
        assert_ne!(opened.session.id, ID);
        let mut binding = source.binding().clone();
        binding.session = opened.session.clone();
        let thread = codex.bind(opened, binding, false)?;
        assert_eq!(thread.bootstrap()["model"], "gpt-5.4-mini");
        assert_eq!(thread.bootstrap()["reasoningEffort"], "low");
        let goal = call(
            &thread,
            "thread/goal/get",
            json!({"threadId":thread.binding().session.id}),
        )
        .await?;
        for key in ["objective", "status", "tokenBudget", "tokensUsed"] {
            assert_eq!(goal["goal"][key], expected["goal"][key], "{key}");
        }
        let mut events = thread.take_events()?;
        while let Ok(event) = events.try_recv() {
            assert_ne!(event["method"], "turn/started");
        }
        assert_eq!(
            call(source, "thread/goal/get", json!({"threadId":ID})).await?,
            *expected
        );
        Ok(())
    }
    .await;
    codex.shutdown().await;
    result
}

async fn call(thread: &Thread, method: &str, params: Value) -> TestResult<Value> {
    match thread.prepare(method, params, true, &BTreeMap::new())? {
        Prepared::Immediate(value) => Ok(value),
        Prepared::Operation(operation) => Ok(thread.execute(operation).await?),
    }
}

fn history(home: &Path) -> TestResult {
    let directory = home.join("sessions/2026/09/11");
    std::fs::create_dir_all(&directory)?;
    let mut rows = vec![json!({"type":"session_meta","payload":{
        "id":ID,"timestamp":"2026-09-11T00:00:00Z","cwd":home,
        "originator":"codex_cli_rs","cli_version":"0.153.4","source":"cli",
        "model_provider":"offline","history_mode":"paginated",
        "base_instructions":{"text":"Synthetic resume regression"}
    }})];
    for index in 1..=3 {
        let turn = format!("01990000-0000-7000-8000-{index:012}");
        rows.push(json!({"type":"event_msg","payload":{"type":"task_started",
            "turn_id":turn,"started_at":1789084800,"model_context_window":128000}}));
        rows.push(json!({"type":"event_msg","payload":{"type":"turn_aborted",
            "turn_id":turn,"reason":"interrupted","completed_at":1789084801}}));
    }
    let mut output = String::new();
    for (ordinal, mut row) in rows.into_iter().enumerate() {
        row["timestamp"] = json!("2026-09-11T00:00:00Z");
        row["ordinal"] = json!(ordinal);
        output.push_str(&serde_json::to_string(&row)?);
        output.push('\n');
    }
    std::fs::write(
        directory.join(format!("rollout-2026-09-11T00-00-00-{ID}.jsonl")),
        output,
    )?;
    Ok(())
}
