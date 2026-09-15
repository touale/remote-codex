use super::{Context, headless, tui};
use remote_codex_client::session::{PermissionPreset, SessionSettings};
use remote_codex_test_support::{
    ProbeResult,
    model::{function, message},
    terminal::Terminal,
};
use serde_json::json;
use std::{collections::BTreeSet, time::Duration};

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let source = headless::open(context, None, false).await?;
    source
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    for marker in [
        "EDIT_SOURCE_FIRST",
        "EDIT_SOURCE_SECOND",
        "EDIT_SOURCE_THIRD",
    ] {
        context.model.script([message(marker)])?;
        headless::turn(&source, false).await?;
    }
    let id = source.session().id.clone();
    source.close().await;
    let before = session_ids(context).await?;
    let requests = context.model.requests()?.len();

    // Cancelling the picker must not create or truncate history.
    let mut first = preview(context, &id, 2).await?;
    first.send(b"q")?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    if session_ids(context).await? != before {
        return Err("cancelled prompt selection changed the session catalogue".into());
    }
    select(&first, 2).await?;
    first.send(b"\r")?;
    first
        .wait_for("You’re continuing from this point in a new conversation")
        .await?;
    // Clear the restored prompt, then quit without submitting the new draft.
    first.send(b"\x01\x0b")?;
    first.type_command("/quit").await?;
    if first.wait_exit().await? != Some(0) {
        return Err(first.diagnostics().into());
    }
    if context.model.requests()?.len() != requests
        || session_ids(context).await? != before
        || first.diagnostics().contains("Failed to")
    {
        return Err(format!(
            "editing the first prompt did not preserve an unsent draft: {}",
            first.diagnostics()
        )
        .into());
    }

    let before = session_ids(context).await?;
    let mut middle = preview(context, &id, 1).await?;
    middle.send(b"\r")?;
    middle
        .wait_for("You’re continuing from this point in a new conversation")
        .await?;
    let branch = created_branch(context, &before).await?;
    if context
        .client
        .sessions()
        .read(&branch, None)
        .await?
        .turns
        .len()
        != 1
    {
        return Err("middle-prompt edit did not retain exactly its preceding history".into());
    }
    // Native prompt editing opens with its own permission selection. A previous
    // session's Full Access grant must not bypass the new session's sandbox.
    middle.send(b"\x01\x0b")?;
    tui::full_access(&middle).await?;
    context.model.script([
        function(
            "exec_command",
            json!({"cmd":"printf once >> edit-command", "workdir":context.environment.workspace}),
            None,
        ),
        message("EDIT_BRANCH_EXECUTED"),
    ])?;
    middle
        .type_command("Run the scripted acceptance task. Revised.")
        .await?;
    middle.wait_for("EDIT_BRANCH_EXECUTED").await?;
    std::fs::write(
        context.environment.args.output.join("tui-edit.txt"),
        middle.diagnostics(),
    )?;
    if context.environment.read("edit-command").await? != "once" {
        return Err("edited prompt did not execute remotely exactly once".into());
    }
    context.environment.stop_service().await?;
    middle.wait_for("Execution environment restored").await?;
    context.model.script([
        function("exec_command", json!({"cmd":"printf restored > edit-recovered", "workdir":context.environment.workspace}), None),
        message("EDIT_BRANCH_RECOVERED"),
    ])?;
    middle
        .type_command("Verify the edited session after recovery.")
        .await?;
    middle.wait_for("EDIT_BRANCH_RECOVERED").await?;
    middle.type_command("/quit").await?;
    if middle.wait_exit().await? != Some(0)
        || !middle
            .diagnostics()
            .contains(&format!("Resume: remote-codex resume {branch}"))
        || context.environment.read("edit-recovered").await? != "restored"
    {
        return Err(format!(
            "edited session lost recovery or exit identity: {}",
            middle.diagnostics()
        )
        .into());
    }
    let mut resumed = tui::start(context, &["resume", &branch])?;
    resumed.wait_for("EDIT_BRANCH_RECOVERED").await?;
    resumed.type_command("/quit").await?;
    if resumed.wait_exit().await? != Some(0) {
        return Err(resumed.diagnostics().into());
    }

    let before = session_ids(context).await?;
    let mut last = preview(context, &id, 0).await?;
    last.send(b"\r")?;
    last.wait_for("You’re continuing from this point in a new conversation")
        .await?;
    let last_id = created_branch(context, &before).await?;
    last.send(b"\x01\x0b")?;
    last.type_command("/quit").await?;
    if last.wait_exit().await? != Some(0)
        || !last
            .diagnostics()
            .contains(&format!("Resume: remote-codex resume {last_id}"))
        || context
            .client
            .sessions()
            .read(&last_id, None)
            .await?
            .turns
            .len()
            != 2
        || context.client.sessions().read(&id, None).await?.turns.len() != 3
    {
        return Err(format!(
            "last-prompt edit changed source history or lost its saved branch: {}",
            last.diagnostics()
        )
        .into());
    }
    std::fs::write(
        context.environment.args.output.join("tui-edit.txt"),
        middle.diagnostics(),
    )?;
    eprintln!(
        "TUI editing: first/middle/last prompts, cancellation, source history, remote execution, recovery and resume identity passed"
    );
    Ok(())
}

async fn preview(context: &Context, id: &str, previous: usize) -> ProbeResult<Terminal> {
    let terminal = tui::start(context, &["resume", id])?;
    terminal.wait_for("EDIT_SOURCE_THIRD").await?;
    select(&terminal, previous).await?;
    Ok(terminal)
}

async fn select(terminal: &Terminal, previous: usize) -> ProbeResult<()> {
    for _ in 0..2 {
        terminal.send(b"\x1b")?;
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    terminal.wait_for("enter to edit message").await?;
    for _ in 0..previous {
        terminal.send(b"\x1b[D")?;
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Ok(())
}

async fn session_ids(context: &Context) -> ProbeResult<BTreeSet<String>> {
    Ok(context
        .client
        .sessions()
        .list(Some("test"), false)
        .await?
        .into_iter()
        .map(|session| session.session.id)
        .collect())
}

async fn created_branch(context: &Context, before: &BTreeSet<String>) -> ProbeResult<String> {
    let after = session_ids(context).await?;
    let mut added = after.difference(before);
    match (added.next(), added.next()) {
        (Some(id), None) if before.is_subset(&after) => Ok(id.clone()),
        _ => Err(
            "prompt editing must add exactly one bound session and preserve existing sessions"
                .into(),
        ),
    }
}
