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
    // The App uses the same native revert, with a projected history and snapshot.
    let history = context.client.sessions().read(&id, None).await?;
    let first_turn = &history
        .turns
        .iter()
        .find(|turn| {
            turn.items
                .iter()
                .any(|item| item.text == "EDIT_SOURCE_FIRST")
        })
        .ok_or("missing first turn")?
        .id;
    let last = &history
        .turns
        .iter()
        .find(|turn| {
            turn.items
                .iter()
                .any(|item| item.text == "EDIT_SOURCE_THIRD")
        })
        .ok_or("missing last turn")?
        .id;
    let reverted = source.revert(last).await?;
    if reverted.history.turns.len() != 2
        || reverted.snapshot.turn.is_some()
        || reverted.snapshot.current_turn.is_some()
    {
        return Err("App revert did not refresh history and clear old turn state".into());
    }
    context.model.script([message("EDIT_SOURCE_THIRD")])?;
    headless::turn(&source, false).await?;
    source.close().await;
    let before = session_ids(context).await?;
    let requests = context.model.requests()?.len();

    // Cancelling selection must not create or truncate history.
    let mut middle = preview(context, &id, "EDIT_SOURCE_THIRD").await?;
    middle.send(b"\x1b")?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    if session_ids(context).await? != before
        || context.client.sessions().read(&id, None).await?.turns.len() != 3
    {
        return Err("cancelled prompt selection changed history".into());
    }
    // Editing the second of three turns must remove it and the following turn.
    select(&middle, 1).await?;
    confirm(&middle).await?;
    let retained = context.client.sessions().read(&id, None).await?;
    if retained.turns.len() != 1
        || retained.turns.first().map(|turn| &turn.id) != Some(first_turn)
        || context.model.requests()?.len() != requests
    {
        return Err("middle-prompt edit did not retain exactly its preceding history".into());
    }
    middle.send(b"\x01\x0b")?;
    context.model.script([
        function(
            "exec_command",
            json!({"cmd":"printf once >> edit-command", "workdir":context.environment.workspace}),
            None,
        ),
        message("EDIT_REVISED_EXECUTED"),
    ])?;
    middle
        .type_command("Run the scripted acceptance task. Revised.")
        .await?;
    middle.wait_for("EDIT_REVISED_EXECUTED").await?;
    if context.environment.read("edit-command").await? != "once" {
        return Err("edited prompt did not execute remotely exactly once".into());
    }
    quit(&mut middle, &id).await?;
    std::fs::write(
        context.environment.args.output.join("tui-edit.txt"),
        middle.diagnostics(),
    )?;

    let requests = context.model.requests()?.len();
    let mut last = preview(context, &id, "EDIT_REVISED_EXECUTED").await?;
    confirm(&last).await?;
    quit(&mut last, &id).await?;
    let retained = context.client.sessions().read(&id, None).await?;
    if retained.turns.len() != 1
        || retained.turns.first().map(|turn| &turn.id) != Some(first_turn)
        || context.model.requests()?.len() != requests
    {
        return Err("last-prompt edit changed retained history or submitted the draft".into());
    }

    let mut first = preview(context, &id, "EDIT_SOURCE_FIRST").await?;
    confirm(&first).await?;
    quit(&mut first, &id).await?;
    if !context
        .client
        .sessions()
        .read(&id, None)
        .await?
        .turns
        .is_empty()
        || context.model.requests()?.len() != requests
        || session_ids(context).await? != before
    {
        return Err("first-prompt edit lost the session identity or submitted its draft".into());
    }
    let mut resumed = tui::start(context, &["resume", &id])?;
    resumed.wait_for("OpenAI Codex").await?;
    quit(&mut resumed, &id).await?;
    eprintln!(
        "TUI editing: cancellation, first/middle/last prompts, App history, exactly-once execution and same-session resume passed"
    );
    Ok(())
}

async fn preview(context: &Context, id: &str, marker: &str) -> ProbeResult<Terminal> {
    let terminal = tui::start(context, &["resume", id])?;
    terminal.wait_for(marker).await?;
    select(&terminal, 0).await?;
    Ok(terminal)
}

async fn select(terminal: &Terminal, previous: usize) -> ProbeResult<()> {
    for _ in 0..2 {
        terminal.send(b"\x1b")?;
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    terminal.wait_for("rewind").await?;
    for _ in 0..previous {
        terminal.send(b"\x1b[D")?;
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Ok(())
}

async fn confirm(terminal: &Terminal) -> ProbeResult<()> {
    terminal.send(b"\r")?;
    terminal
        .wait_for("Conversation reverted to this point")
        .await
}

async fn quit(terminal: &mut Terminal, id: &str) -> ProbeResult<()> {
    terminal.send(b"\x01\x0b")?;
    terminal.type_command("/quit").await?;
    if terminal.wait_exit().await? != Some(0)
        || !terminal
            .diagnostics()
            .contains(&format!("Resume: remote-codex resume {id}"))
    {
        return Err(terminal.diagnostics().into());
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
