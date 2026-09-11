//! Contract checks exercise native state, not a mocked copy of these mappings.
use remote_codex_adapter::{engine::Engine, goals};
use remote_codex_core::goals::{CollaborationMode, GoalStatus};
use serde_json::json;

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; uses a temporary Codex home, no model turn"]
async fn native_plan_and_persisted_goal_contract() -> Result<(), Box<dyn std::error::Error>> {
    let program = std::path::PathBuf::from(std::env::var("REMOTE_CODEX_TEST_BINARY")?);
    let home = tempfile::tempdir()?;
    std::fs::write(
        home.path().join("config.toml"),
        "model=\"gpt-6-astra\"\n[analytics]\nenabled=false\n",
    )?;
    let (engine, _) = Engine::local(&program, home.path()).await?;
    let result=async {
        let thread=engine.call("thread/start",json!({"cwd":home.path(),"sandbox":"read-only","approvalPolicy":"on-request"})).await?;
        let id=thread["thread"]["id"].as_str().ok_or("thread missing")?;
        let mut events=engine.subscribe();
        engine.call("thread/settings/update",json!({"threadId":id,"collaborationMode":goals::mode(&json!({"model":"gpt-6-astra","effort":"medium"}),CollaborationMode::Plan)})).await?;
        let settings=tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {let event=events.recv().await?;if event["method"] == "thread/settings/updated" {return Ok::<_,tokio::sync::broadcast::error::RecvError>(event["params"]["threadSettings"].clone());}}
        }).await??;
        assert_eq!(settings["collaborationMode"]["mode"],"plan");
        assert_eq!(settings["model"],"gpt-6-astra");
        assert_eq!(settings["sandboxPolicy"]["type"],"readOnly");
        let response=engine.call("thread/goal/set",json!({"threadId":id,"objective":"Review the isolated contract","status":"paused","tokenBudget":40000})).await?;
        let goal=goals::read(&response)?.ok_or("goal missing")?;
        assert_eq!(goal.status,GoalStatus::Paused);
        assert_eq!(goal.token_budget,Some(40000));
        engine.call("thread/goal/set",json!({"threadId":id,"tokenBudget":50000})).await?;
        let updated=goals::read(&engine.call("thread/goal/get",json!({"threadId":id})).await?)?.ok_or("goal missing")?;
        assert_eq!(updated.objective,goal.objective);
        assert_eq!(updated.tokens_used,goal.tokens_used);
        assert_eq!(updated.token_budget,Some(50000));
        engine.call("thread/goal/clear",json!({"threadId":id})).await?;
        assert!(goals::read(&engine.call("thread/goal/get",json!({"threadId":id})).await?)?.is_none());
        Ok::<_,Box<dyn std::error::Error>>(())
    }.await;
    engine.shutdown().await;
    result
}
