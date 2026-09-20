use remote_codex_adapter::{engine::Engine, goals, program::Launch};
use remote_codex_client::application::GoalStatus;
use remote_codex_test_support::{
    ProbeResult,
    model::{ModelFixture, function, message},
};
use serde_json::json;

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; isolated Codex and loopback model, no remote execution"]
async fn native_goal_resume_runs_and_stops_at_completion() -> ProbeResult<()> {
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
        function("update_goal", json!({"status":"complete"}), None),
        message("The isolated goal is complete."),
    ])?;
    let (engine, _) = Engine::local(&program, home.path()).await?;
    let result=async {
        let started=engine.call("thread/start",json!({"cwd":home.path(),"sandbox":"read-only","approvalPolicy":"on-request"})).await?;
        let id=started["thread"]["id"].as_str().ok_or("thread missing")?;
        engine.call("thread/goal/set",json!({"threadId":id,"objective":"Mark this isolated test goal complete.","status":"paused","tokenBudget":100})).await?;
        assert!(model.requests()?.is_empty(),"a paused goal started inference");
        let mut events=engine.subscribe();
        engine.call("thread/goal/set",json!({"threadId":id,"status":"active"})).await?;
        tokio::time::timeout(std::time::Duration::from_secs(20),async {
            loop {let e=events.recv().await?;if e["method"] == "turn/completed" {return Ok::<_,tokio::sync::broadcast::error::RecvError>(());}}
        }).await??;
        let goal=goals::read(&engine.call("thread/goal/get",json!({"threadId":id})).await?)?.ok_or("goal missing")?;
        assert_eq!(goal.status,GoalStatus::Complete);
        assert!(goal.tokens_used > 0);
        assert!(!model.requests()?.is_empty());
        eprintln!("native goal: paused goal did not run; resume triggered inference, completion and usage were persisted");
        Ok::<_,Box<dyn std::error::Error+Send+Sync>>(())
    }.await;
    engine.shutdown().await;
    result
}
