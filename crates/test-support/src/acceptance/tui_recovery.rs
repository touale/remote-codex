use super::{Context, tui};
use remote_codex_test_support::{
    ProbeResult,
    model::{MARKER, function},
};
use serde_json::json;
use std::time::Duration;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let mut terminal = tui::start(context, &[])?;
    terminal.wait_for("gpt-6-astra").await?;
    tui::full_access(&terminal).await?;
    context.model.script([function("exec_command", json!({"cmd":"printf once >> tui-restart-count; sleep 20", "workdir":context.environment.workspace,"yield_time_ms":10000}), None)])?;
    terminal
        .type_command("Run the fixed TUI recovery task.")
        .await?;
    tokio::time::timeout(Duration::from_secs(20), async {
        while context.environment.absent("tui-restart-count").await? {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    let id = context
        .client
        .sessions()
        .list(Some("test"), false)
        .await?
        .first()
        .ok_or("TUI session missing")?
        .session
        .id
        .clone();
    context.environment.stop_service().await?;
    terminal.wait_for("Execution environment restored").await?;
    terminal.wait_for(MARKER).await?;
    context.model.script([function("exec_command", json!({"cmd":"printf restored > tui-restart-after", "workdir":context.environment.workspace}), None)])?;
    let before = context.model.requests()?.len();
    terminal
        .type_command("Verify execution after recovery.")
        .await?;
    context.model.wait_for(before + 2).await?;
    if context.environment.read("tui-restart-count").await? != "once"
        || context.environment.read("tui-restart-after").await? != "restored"
    {
        return Err("TUI recovery replayed or lost execution".into());
    }

    // A changed enabled set blocks recovery before stale executor permissions
    // are consulted, even when the TUI still sends its Full Access selection.
    let skill = context.environment.home.join("skills/recovery-probe");
    std::fs::create_dir_all(&skill)?;
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: recovery-probe\ndescription: Verify explicit Skill reload after interruption.\n---\nInspect existing results before continuing.\n",
    )?;
    context.environment.stop_service().await?;
    terminal.wait_for("SKILLS_CHANGED").await?;
    let blocked = context.model.requests()?.len();
    terminal
        .type_command("Continue after the interruption.")
        .await?;
    terminal.wait_for("Failed to start turn").await?;
    let diagnostics = terminal.diagnostics();
    if !diagnostics
        .split_once("Failed to start turn")
        .is_some_and(|(_, error)| error.contains("SKILLS_CHANGED"))
        || context.model.requests()?.len() != blocked
    {
        return Err(format!(
            "blocked TUI recovery lost its cause or submitted a turn: {diagnostics}"
        )
        .into());
    }
    terminal.type_command("/quit").await?;
    let status = terminal.wait_exit().await?;
    let diagnostics = terminal.diagnostics();
    std::fs::write(
        context.environment.args.output.join("tui-recovery.txt"),
        &diagnostics,
    )?;
    if status != Some(0) || !diagnostics.contains(&format!("Resume: remote-codex resume {id}")) {
        return Err(format!("TUI recovery failed: {diagnostics}").into());
    }

    let mut resumed = tui::start(context, &["resume", &id])?;
    resumed.wait_for("gpt-6-astra").await?;
    context.model.script([function(
        "exec_command",
        json!({"cmd":"printf resumed > tui-skill-resume", "workdir":context.environment.workspace}),
        None,
    )])?;
    resumed
        .type_command("Verify execution with the current Skills.")
        .await?;
    context.model.wait_for(blocked + 2).await?;
    if context.environment.read("tui-skill-resume").await? != "resumed"
        || !context
            .model
            .requests()?
            .last()
            .is_some_and(|r| r.to_string().contains("recovery-probe/SKILL.md"))
    {
        return Err(
            "explicit resume did not apply the current Skills and restore execution".into(),
        );
    }
    resumed.type_command("/quit").await?;
    if resumed.wait_exit().await? != Some(0) {
        return Err(resumed.diagnostics().into());
    }
    std::fs::write(
        context.environment.args.output.join("tui-skill-resume.txt"),
        resumed.diagnostics(),
    )?;
    eprintln!(
        "TUI recovery: restart preserved execution; changed Skills blocked new turns with the correct cause; explicit resume applied them"
    );
    Ok(())
}
