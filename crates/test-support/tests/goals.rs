use remote_codex_adapter::{engine::Engine, goals, program::Launch};
use remote_codex_client::application::GoalStatus;
use remote_codex_test_support::{
    ProbeResult,
    model::{ModelFixture, function, message},
};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; isolated Codex and loopback model, no remote execution"]
async fn native_goal_resume_continues_after_a_running_turn_without_restarting_it() -> ProbeResult<()>
{
    let program = Launch::new(std::env::var("REMOTE_CODEX_TEST_BINARY")?);
    let home = tempfile::tempdir()?;
    let model = ModelFixture::start().await?;
    std::fs::write(
        home.path().join("config.toml"),
        format!(
            "model=\"fixture-model\"\nmodel_provider=\"fixture\"\n[model_providers.fixture]\nname=\"Goal contract\"\nbase_url={}\nwire_api=\"responses\"\nrequires_openai_auth=false\n[analytics]\nenabled=false\n",
            serde_json::to_string(&model.base_url)?
        ),
    )?;
    model.script([
        function(
            "exec_command",
            json!({
                "cmd": "printf goal-contract",
                "workdir": home.path(),
                "sandbox_permissions": "require_escalated",
                "justification": "Hold the isolated turn at an approval boundary"
            }),
            None,
        ),
        message("The existing turn is complete."),
        function("update_goal", json!({"status":"complete"}), None),
        message("The isolated goal is complete."),
    ])?;
    let (engine, _) = Engine::local(&program, home.path()).await?;
    let result = async {
        let started = engine
            .call(
                "thread/start",
                json!({"cwd": home.path(), "sandbox": "read-only", "approvalPolicy": "on-request"}),
            )
            .await?;
        let id = started["thread"]["id"].as_str().ok_or("thread missing")?;
        engine
            .call(
                "thread/goal/set",
                json!({
                    "threadId": id,
                    "objective": "Mark this isolated test goal complete.",
                    "status": "paused",
                    "tokenBudget": 100
                }),
            )
            .await?;
        assert!(model.requests()?.is_empty(), "a paused goal started inference");
        let mut events = engine.subscribe();
        let turn = engine
            .call(
                "turn/start",
                json!({
                    "threadId": id,
                    "input": [{"type": "text", "text": "Run the isolated approval command."}]
                }),
            )
            .await?;
        let approval = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let event = events.recv().await?;
                if event["method"] == "item/commandExecution/requestApproval" {
                    return Ok::<_, tokio::sync::broadcast::error::RecvError>(event["id"].clone());
                }
            }
        })
        .await??;
        engine
            .call("thread/goal/set", json!({"threadId": id, "status": "active"}))
            .await?;
        let resumed = goals::read(
            &engine.call("thread/goal/get", json!({"threadId": id})).await?,
        )?
        .ok_or("goal missing")?;
        assert_eq!(resumed.status, GoalStatus::Active);
        assert_eq!(resumed.objective, "Mark this isolated test goal complete.");
        assert_eq!(resumed.token_budget, Some(100));
        assert_eq!(
            model.requests()?.len(),
            1,
            "resume started another inference during the existing turn"
        );
        engine.send(json!({"id": approval, "result": {"decision": "accept"}}))?;
        tokio::time::timeout(Duration::from_secs(20), async {
            let mut completed = Vec::new();
            loop {
                let e = events.recv().await?;
                if e["method"] == "turn/completed" {
                    completed.push(e["params"]["turn"]["id"].clone());
                    if completed.len() == 2 {
                        assert_eq!(completed[0], turn["turn"]["id"]);
                        assert_ne!(completed[0], completed[1]);
                        return Ok::<_, tokio::sync::broadcast::error::RecvError>(());
                    }
                }
            }
        })
        .await??;
        let goal = goals::read(
            &engine.call("thread/goal/get", json!({"threadId": id})).await?,
        )?
        .ok_or("goal missing")?;
        assert_eq!(goal.status, GoalStatus::Complete);
        assert!(goal.tokens_used > 0);
        assert_eq!(model.requests()?.len(), 4);
        eprintln!("native goal: resumed during a running turn; original turn completed once, then goal completed with budget and usage preserved");
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;
    engine.shutdown().await;
    result
}
