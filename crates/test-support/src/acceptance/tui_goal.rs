//! Native bracketed paste must survive local goal storage and remote execution.
use super::{Context, tui};
use remote_codex_test_support::{
    ProbeResult,
    model::{function, message},
};
use serde_json::{Value, json};
use std::time::Duration;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let mut terminal = tui::start(context, &[])?;
    terminal.wait_for("fixture-model").await?;
    tui::full_access(&terminal).await?;
    let text = format!(
        "{}\nEND_OF_LONG_GOAL_验证尾部完整",
        "Review the isolated goal. 长目标内容。\n".repeat(250)
    );
    let before = context.model.requests()?.len();
    context.model.script([
        function("read_goal_attachments", json!({}), None),
        function("exec_command", json!({"cmd":"printf goal-paste-ok > goal-paste-result","workdir":context.environment.workspace}), None),
        function("update_goal", json!({"status":"complete"}), None),
        message("GOAL_PASTE_COMPLETE"),
    ])?;
    for byte in b"/goal " {
        terminal.send(&[*byte])?;
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    terminal.send(format!("\x1b[200~{text}\x1b[201~").as_bytes())?;
    tokio::time::sleep(Duration::from_millis(300)).await;
    terminal.send(b"\r")?;
    terminal.wait_for("GOAL_PASTE_COMPLETE").await?;
    let requests = context.model.requests()?;
    let read = requests
        .get(before + 1)
        .ok_or("attachment tool result missing")?;
    if !contains_text(&read["input"], &text) {
        return Err(format!(
            "model did not receive the complete pasted goal: {}",
            read["input"]
        )
        .into());
    }
    if context.environment.read("goal-paste-result").await? != "goal-paste-ok" {
        return Err("remote command did not run after reading the local goal attachment".into());
    }
    let sessions = context.client.sessions().list(Some("test"), false).await?;
    let session = sessions.first().ok_or("goal session missing")?;
    terminal.type_command("/quit").await?;
    if terminal.wait_exit().await? != Some(0) {
        return Err(terminal.diagnostics().into());
    }
    let diagnostics = terminal.diagnostics();
    std::fs::write(
        context.environment.args.output.join("goal-paste.txt"),
        &diagnostics,
    )?;
    if diagnostics.contains("Could not create goal attachment") || diagnostics.contains("Failed to")
    {
        return Err("native goal composer reported a failure".into());
    }
    let mut resumed = tui::start(context, &["resume", &session.session.id])?;
    resumed.wait_for("GOAL_PASTE_COMPLETE").await?;
    resumed.type_command("/goal").await?;
    resumed.wait_for("pasted-text-1.txt").await?;
    resumed.type_command("/quit").await?;
    if resumed.wait_exit().await? != Some(0) {
        return Err(resumed.diagnostics().into());
    }
    if context.model.requests()?.len() != requests.len() {
        return Err("resuming a completed goal unexpectedly restarted execution".into());
    }
    eprintln!(
        "long /goal paste: local attachment text intact, remote execution succeeded, goal persisted across resume"
    );
    Ok(())
}

// Native function outputs can contain JSON text inside content blocks.
fn contains_text(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(text) => {
            text == expected
                || serde_json::from_str::<Value>(text)
                    .ok()
                    .is_some_and(|parsed| parsed != *value && contains_text(&parsed, expected))
        }
        Value::Array(values) => values.iter().any(|value| contains_text(value, expected)),
        Value::Object(values) => values.values().any(|value| contains_text(value, expected)),
        _ => false,
    }
}
