use super::Context;
use portable_pty::CommandBuilder;
use remote_codex_test_support::{
    ProbeResult,
    model::{MARKER, function},
    terminal::Terminal,
};
use std::time::Duration;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let mut terminal = start(context, &[])?;
    terminal.wait_for("fixture-model").await?;
    terminal.type_command("/model").await?;
    terminal.wait_for("Select Model and Effort").await?;
    for _ in 0..2 {
        terminal.send(b"\r")?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let config: toml::Value = toml::from_str(&std::fs::read_to_string(
        context.environment.home.join("config.toml"),
    )?)?;
    if config.get("model").and_then(toml::Value::as_str) != Some("gpt-6-astra")
        || config
            .get("model_reasoning_effort")
            .and_then(toml::Value::as_str)
            != Some("low")
    {
        return Err(format!(
            "TUI did not persist model preferences: {}",
            terminal.diagnostics()
        )
        .into());
    }
    full_access(&terminal).await?;
    let before = context.model.requests()?.len();
    context.model.script([function(
        "exec_command",
        serde_json::json!({"cmd":"printf tui-full-access > tui-full-access", "workdir":context.environment.workspace}),
        None,
    )])?;
    terminal.type_command("Run the scripted TUI task.").await?;
    context.model.wait_for(before + 2).await?;
    terminal.wait_for(MARKER).await?;
    if context.environment.read("tui-full-access").await? != "tui-full-access" {
        return Err("TUI Full Access was not applied to execution".into());
    }
    for request in &context.model.requests()?[before..] {
        if request["model"] != "gpt-6-astra"
            || request.pointer("/reasoning/effort") != Some(&serde_json::json!("low"))
        {
            return Err("TUI model selection did not reach model requests".into());
        }
    }
    let sessions = context.client.sessions().list(Some("test"), false).await?;
    let session = sessions.first().ok_or("TUI session missing")?;
    terminal.type_command("/quit").await?;
    if terminal.wait_exit().await? != Some(0) {
        return Err(terminal.diagnostics().into());
    }
    let diagnostics = terminal.diagnostics();
    std::fs::write(
        context.environment.args.output.join("tui.txt"),
        &diagnostics,
    )?;
    if diagnostics.contains("Failed to update")
        || diagnostics.contains("Failed to save")
        || !diagnostics.contains(&format!(
            "Resume: remote-codex resume {}",
            session.session.id
        ))
    {
        return Err("TUI settings or exit recovery failed".into());
    }
    let mut resumed = start(context, &["resume", &session.session.id])?;
    resumed.wait_for(MARKER).await?;
    resumed.send(b"\x03")?;
    tokio::time::sleep(Duration::from_millis(350)).await;
    resumed.send(b"\x03")?;
    if resumed.wait_exit().await? != Some(0) {
        return Err(resumed.diagnostics().into());
    }
    if context.model.requests()?.len() != before + 2 {
        return Err("TUI resume replayed the prompt".into());
    }
    if !resumed.diagnostics().contains(&format!(
        "Resume: remote-codex resume {}",
        session.session.id
    )) {
        return Err("Ctrl+C exit lost recovery guidance".into());
    }
    std::fs::write(
        context.environment.args.output.join("resume.txt"),
        resumed.diagnostics(),
    )?;
    eprintln!("TUI: model preferences, Full Access, exit guidance and resume passed");
    Ok(())
}

pub(super) fn start(context: &Context, args: &[&str]) -> ProbeResult<Terminal> {
    let mut command = CommandBuilder::new(&context.environment.args.cli);
    command.arg("--data-dir");
    command.arg(&context.environment.state);
    command.args(["-n", "test"]);
    if let Some(config) = &context.environment.args.ssh_config {
        command.arg("--ssh-config");
        command.arg(config);
    }
    if args.is_empty() {
        command.args(["--path", &context.environment.workspace]);
    }
    command.args(args);
    command.env("TERM", "xterm-256color");
    Terminal::spawn(command)
}

pub(super) async fn full_access(terminal: &Terminal) -> ProbeResult<()> {
    terminal.type_command("/permissions").await?;
    terminal.wait_for("Update Model Permissions").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    terminal.send(b"\x1b[B\x1b[B\r")?;
    terminal.wait_for("Yes, continue anyway").await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    terminal.send(b"\r")?;
    terminal
        .wait_for("Permissions updated to Full Access")
        .await?;
    Ok(())
}
