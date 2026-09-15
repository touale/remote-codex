//! A paused goal and an empty interrupted turn must not make a resumed TUI busy.
use super::{Context, headless, tui};
use remote_codex_client::application::{
    CollaborationMode, GoalAction, GoalStatus, SessionSettings,
};
use remote_codex_test_support::{
    ProbeResult,
    model::{function, message},
};
use serde_json::{Value, json};
use std::io::Write;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    for via_command in [false, true] {
        resume(context, via_command).await?;
    }
    Ok(())
}

async fn resume(context: &Context, via_command: bool) -> ProbeResult<()> {
    let session = headless::open(context, None, false).await?;
    context.model.script([message("RESUME_HISTORY_MARKER")])?;
    headless::turn(&session, false).await?;
    session
        .settings(SessionSettings {
            mode: Some(CollaborationMode::Plan),
            ..Default::default()
        })
        .await?;
    let goal = session
        .goal(GoalAction::Set {
            objective: "Continue the isolated paused goal once".into(),
            token_budget: Some(40000),
        })
        .await?
        .ok_or("goal missing")?;
    session
        .settings(SessionSettings {
            mode: Some(CollaborationMode::Agent),
            ..Default::default()
        })
        .await?;
    let id = session.session().id.clone();
    session.close().await;
    append_interrupted_turn(context, &id).await?;
    let before = context.model.requests()?.len();

    let mut failed = tui::start(
        context,
        &["resume", &id, "--", "--invalid-resume-test-flag"],
    )?;
    failed.wait_for("Resume: remote-codex resume").await?;
    if failed.wait_exit().await?.is_none_or(|code| code == 0)
        || !failed
            .diagnostics()
            .contains(&format!("Resume: remote-codex resume {id}"))
        || context.model.requests()?.len() != before
    {
        return Err(format!(
            "frontend failure started a goal or lost recovery guidance: {}",
            failed.diagnostics()
        )
        .into());
    }

    let mut paused = tui::start(context, &["resume", &id])?;
    paused.wait_for("Resume paused goal?").await?;
    paused.wait_for("Leave paused").await?;
    paused.send(b"\x1b[B\r")?;
    paused.wait_for("RESUME_HISTORY_MARKER").await?;
    paused.type_command("/status").await?;
    paused.send(b"\x1b")?;
    if paused.interrupt().await? != Some(0) {
        return Err(format!("paused resume did not exit: {}", paused.diagnostics()).into());
    }
    let output = paused.diagnostics();
    std::fs::write(
        context.environment.args.output.join("paused-resume.txt"),
        &output,
    )?;
    if output.contains("Working (")
        || output.contains("Full-history hydration")
        || output.contains("Resume the paused goal?")
        || output.contains("Paused goal:")
        || context.model.requests()?.len() != before
    {
        return Err("paused resume became busy, hydrated all history or started inference".into());
    }
    let inspect = headless::open(context, Some(id.clone()), false).await?;
    let kept = inspect.snapshot()?.goal.ok_or("paused goal lost")?;
    inspect.close().await;
    if kept.status != GoalStatus::Paused
        || kept.tokens_used != goal.tokens_used
        || kept.time_used_seconds != goal.time_used_seconds
        || kept.token_budget != goal.token_budget
    {
        return Err("declining resume changed the goal".into());
    }

    context.model.script([
        function("update_goal", json!({"status":"complete"}), None),
        message("RESUMED_GOAL_COMPLETE"),
    ])?;
    let mut resumed = tui::start(context, &["resume", &id])?;
    resumed.wait_for("Resume paused goal?").await?;
    resumed.wait_for("Leave paused").await?;
    if via_command {
        resumed.send(b"\x1b[B\r")?;
        resumed.type_command("/goal resume").await?;
    } else {
        resumed.send(b"\r")?;
    }
    resumed.wait_for("RESUMED_GOAL_COMPLETE").await?;
    resumed.type_command("/quit").await?;
    if resumed.wait_exit().await? != Some(0) || context.model.requests()?.len() != before + 2 {
        return Err(format!(
            "goal resume duplicated or failed: {}",
            resumed.diagnostics()
        )
        .into());
    }
    let inspect = headless::open(context, Some(id), false).await?;
    let completed = inspect.snapshot()?.goal.ok_or("resumed goal lost")?;
    inspect.close().await;
    if completed.status != GoalStatus::Complete
        || completed.objective != goal.objective
        || completed.token_budget != goal.token_budget
        || completed.tokens_used < goal.tokens_used
    {
        return Err("resume replaced the existing goal".into());
    }
    eprintln!(
        "TUI resume (slash command: {via_command}): native goal choice, idle interrupted history, Ctrl+C and failed frontend cleanup passed"
    );
    Ok(())
}

async fn append_interrupted_turn(context: &Context, id: &str) -> ProbeResult<()> {
    // Only this fixture's closed rollout is changed; native resume rebuilds its projection.
    let db = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(context.environment.home.join("state_5.sqlite"))
            .read_only(true),
    )
    .await?;
    let path: String = sqlx::query_scalar("SELECT rollout_path FROM threads WHERE id=?")
        .bind(id)
        .fetch_one(&db)
        .await?;
    db.close().await;
    let path = std::path::Path::new(&path).canonicalize()?;
    if !path.starts_with(context.environment.home.canonicalize()?) {
        return Err("rollout is outside the isolated Codex home".into());
    }
    let rows: Vec<Value> = std::fs::read_to_string(&path)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let ordinal = rows
        .iter()
        .filter_map(|r| r["ordinal"].as_u64())
        .max()
        .ok_or("ordinal missing")?
        + 1;
    let turn = "01a00000-0000-7000-8000-000000000001";
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    for (offset, payload) in [
        json!({"type":"task_started","turn_id":turn,"model_context_window":128000}),
        json!({"type":"turn_aborted","turn_id":turn,"reason":"interrupted"}),
    ]
    .into_iter()
    .enumerate()
    {
        writeln!(
            file,
            "{}",
            json!({"type":"event_msg","timestamp":"2026-09-12T00:00:00Z",
            "ordinal":ordinal + offset as u64,"payload":payload})
        )?;
    }
    Ok(())
}
