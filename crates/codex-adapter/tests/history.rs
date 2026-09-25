//! Synthetic history only: no user Codex home, model requests, or remote server.
use remote_codex_adapter::{
    engine::Engine,
    program::Launch,
    thread::{Codex, OpenSource, OpenThread, history},
};
use remote_codex_core::session::SessionBinding;
use serde_json::json;
use std::{
    collections::{BTreeMap, HashSet},
    io::{BufRead, Write},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const ID: &str = "01990000-0000-7000-8000-000000000020";
const TURN: &str = "01990000-0000-7000-8000-000000000002";

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; synthetic history exceeding 64 MiB"]
async fn large_history_pages_preserve_items_and_defer_tool_output() -> TestResult {
    let program = Launch::new(std::env::var("REMOTE_CODEX_TEST_BINARY")?);
    let home = tempfile::tempdir()?;
    let cwd = home.path().to_str().ok_or("invalid test path")?;
    std::fs::write(
        home.path().join("config.toml"),
        "model=\"gpt-6-astra\"\nmodel_provider=\"offline\"\n[model_providers.offline]\nname=\"Offline\"\nbase_url=\"http://127.0.0.1:1/v1\"\nwire_api=\"responses\"\nrequires_openai_auth=false\n[analytics]\nenabled=false\n",
    )?;
    let directory = home.path().join("sessions/2026/09/11");
    std::fs::create_dir_all(&directory)?;
    let fixture = directory.join(format!("rollout-2026-09-11T00-00-00-{ID}.jsonl"));
    let mut file = std::io::BufWriter::new(std::fs::File::create(&fixture)?);
    let mut ordinal = 0;
    let mut row = |kind: &str, payload: serde_json::Value| -> std::io::Result<()> {
        writeln!(
            file,
            "{}",
            json!({"timestamp":"2026-09-11T00:00:00Z","ordinal":ordinal,"type":kind,"payload":payload})
        )?;
        ordinal += 1;
        Ok(())
    };
    row(
        "session_meta",
        json!({"id":ID,"timestamp":"2026-09-11T00:00:00Z","cwd":cwd,
        "originator":"codex_cli_rs","cli_version":"fixture","source":"cli","model_provider":"offline",
        "history_mode":"paginated","base_instructions":{"text":"Synthetic history"}}),
    )?;
    let empty = "01990000-0000-7000-8000-000000000001";
    for turn in [empty, TURN] {
        row(
            "event_msg",
            json!({"type":"task_started","turn_id":turn,"model_context_window":128000}),
        )?;
        if turn == TURN {
            row(
                "event_msg",
                json!({"type":"item_completed","thread_id":ID,"turn_id":turn,
                "item":{"type":"UserMessage","id":"user","content":[{"type":"text","text":"Synthetic task"}]}}),
            )?;
            for i in 0..34 {
                row(
                    "event_msg",
                    json!({"type":"item_completed","thread_id":ID,"turn_id":turn,
                    "item":{"type":"McpToolCall","id":format!("tool-{i:02}"),"server":"fixture","tool":"output",
                    "arguments":{},"readOnlyHint":true,"status":"completed","duration":{"secs":1,"nanos":0},
                    "result":{"content":[{"type":"text","text":"x".repeat(2 * 1024 * 1024)}],"isError":false}}}),
                )?;
            }
        }
        row(
            "event_msg",
            json!({"type":"turn_aborted","turn_id":turn,"reason":"interrupted","completed_at":1789084801}),
        )?;
    }
    file.flush()?;
    let codex = Codex::start(&program, home.path()).await?;
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
        server_id: "fixture".into(),
        remote_identity: "fixture".into(),
        environment_id: "local".into(),
        codex_home: cwd.into(),
        codex_version: "fixture".into(),
        execution_mode: "sandboxed".into(),
        revision: 1,
    };
    let thread = codex.bind(opened, binding.clone(), false)?;
    let first = thread.history().await;
    codex.shutdown().await;
    let mut page = first?;
    assert!(page.turns[0].items_before);
    drop(file);
    let ordinal = std::io::BufReader::new(std::fs::File::open(&fixture)?)
        .lines()
        .filter_map(|line| {
            serde_json::from_str::<serde_json::Value>(&line.ok()?).ok()?["ordinal"].as_u64()
        })
        .max()
        .ok_or("missing fixture ordinal")?
        + 1;
    let mut file = std::fs::OpenOptions::new().append(true).open(&fixture)?;
    // A new turn between page requests must not redirect the in-turn cursor.
    let newer = "01990000-0000-7000-8000-000000000003";
    for (index, payload) in [
        json!({"type":"task_started","turn_id":newer,"model_context_window":128000}),
        json!({"type":"turn_aborted","turn_id":newer,"reason":"interrupted"}),
    ]
    .into_iter()
    .enumerate()
    {
        writeln!(
            file,
            "{}",
            json!({"timestamp":"2026-09-11T00:00:00Z","ordinal":ordinal+index as u64,
            "type":"event_msg","payload":payload})
        )?;
    }
    file.flush()?;
    let (indexer, _) = Engine::local(&program, home.path()).await?;
    let refreshed = indexer
        .call("thread/resume", json!({"threadId":ID,"excludeTurns":true}))
        .await;
    let newest = indexer
        .call(
            "thread/turns/list",
            json!({"threadId":ID,"itemsView":"notLoaded","limit":1}),
        )
        .await;
    indexer.shutdown().await;
    refreshed?;
    assert_eq!(newest?["data"][0]["id"], newer);
    let source = page.turns[0].items[0]
        .tool
        .as_ref()
        .and_then(|t| t.output_source.clone())
        .ok_or("missing deferred output")?;
    let mut ids = HashSet::new();
    let mut empty_seen = false;
    loop {
        assert!(page.turns.iter().map(|t| t.items.len()).sum::<usize>() <= 16);
        assert!(serde_json::to_vec(&page)?.len() < 64 * 1024);
        for turn in &page.turns {
            assert_ne!(turn.id, newer);
            empty_seen |= turn.id == empty;
            for item in &turn.items {
                assert!(ids.insert(item.id.clone()), "duplicate history item");
                if let Some(tool) = &item.tool {
                    assert!(tool.output.is_empty());
                    assert!(tool.output_source.is_some());
                }
            }
        }
        let Some(cursor) = page.next_cursor else {
            break;
        };
        page = history::read(&program, &binding, Some(&cursor)).await?;
    }
    assert_eq!(ids.len(), 35);
    assert!(ids.contains("user") && empty_seen);
    assert_eq!(
        history::read_output(&program, &binding, &source)
            .await?
            .len(),
        2 * 1024 * 1024
    );
    assert!(
        history::read(&program, &binding, Some("invalid cursor"))
            .await
            .is_err()
    );
    // Verify the original request fails on the very same synthetic history.
    let (engine, _) = Engine::local(&program, home.path()).await?;
    let old = engine
        .call(
            "thread/turns/list",
            json!({"threadId":ID,"itemsView":"full","limit":20}),
        )
        .await;
    engine.shutdown().await;
    assert_eq!(
        old.err()
            .ok_or("expected oversized original response")?
            .code,
        "CODEX_PROTOCOL_ERROR"
    );
    Ok(())
}
