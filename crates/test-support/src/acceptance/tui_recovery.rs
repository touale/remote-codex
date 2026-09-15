use super::{Context, tui};
use remote_codex_test_support::{
    ProbeResult,
    model::{MARKER, function},
};
use serde_json::json;
use std::time::Duration;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let skill = context.environment.home.join("skills/recovery-probe");
    std::fs::create_dir_all(&skill)?;
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: recovery-probe\ndescription: Verify Skill resources across recovery.\n---\nInspect existing results before continuing.\n",
    )?;
    let mut terminal = tui::start(context, &[])?;
    terminal.wait_for("OpenAI Codex").await?;
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
    .await
    .map_err(|_| format!("TUI restart task did not start: {}", terminal.diagnostics()))??;
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
    // Native plugin catalogs can grow after initial discovery. New entries
    // must not invalidate the resources already bound to a running session.
    let late_skill = context.environment.home.join("skills/late-discovery");
    std::fs::create_dir_all(&late_skill)?;
    std::fs::write(
        late_skill.join("SKILL.md"),
        "---\nname: late-discovery\ndescription: Skill discovered after startup.\n---\nInspect the existing workspace.\n",
    )?;
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

    if let Err(error) = super::tui_outage::exercise(context, &terminal).await {
        return Err(format!("TUI outage: {error}: {}", terminal.diagnostics()).into());
    }

    // A real change to a previously bound resource still blocks recovery.
    let skill_directories = format!(
        "cd {} && find skills skill-staging -type d | sort",
        super::environment::quote(&format!("{}/service", context.environment.workspace))?
    );
    let before_upload = context.environment.command(&skill_directories).await?;
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: recovery-probe\ndescription: Verify explicit Skill reload after interruption.\n---\nInspect existing results before continuing.\n",
    )?;
    context.environment.stop_service().await?;
    terminal.wait_for("SKILLS_CHANGED").await?;
    let blocked = context.model.requests()?.len();
    let previous_errors = terminal
        .diagnostics()
        .matches("Failed to start turn")
        .count();
    terminal
        .type_command("Continue after the interruption.")
        .await?;
    tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            let output = terminal.diagnostics();
            if output.matches("Failed to start turn").count() > previous_errors
                && output
                    .rsplit_once("Failed to start turn")
                    .is_some_and(|(_, error)| error.contains("SKILLS_CHANGED"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| {
        format!(
            "blocked TUI recovery lost its cause: {}",
            terminal.diagnostics()
        )
    })?;
    if context.model.requests()?.len() != blocked {
        return Err("blocked TUI recovery submitted a turn".into());
    }
    if context.environment.command(&skill_directories).await? != before_upload {
        return Err("changed Skill resources were uploaded before recovery rejected them".into());
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
    resumed.wait_for("OpenAI Codex").await?;
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
        "TUI recovery: catalog additions preserved execution; changed resources blocked uploads and new turns; explicit resume applied them"
    );
    Ok(())
}
