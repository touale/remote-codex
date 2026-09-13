use super::{Context, ssh, tui};
use portable_pty::CommandBuilder;
use remote_codex_test_support::{
    ProbeResult,
    model::{MARKER, function},
    terminal::Terminal,
};
use std::{process::Stdio, time::Duration};
use tokio::io::AsyncWriteExt;
use zeroize::Zeroizing;

pub(super) async fn exercise(context: &Context, password: &Zeroizing<String>) -> ProbeResult<()> {
    let result = exercise_saved(context, password).await;
    forget(context).await?;
    result?;
    eprintln!(
        "saved password: separate CLI reuse, rejected rotation, actual master recovery and Keychain cleanup passed"
    );
    Ok(())
}

async fn forget(context: &Context) -> ProbeResult<()> {
    // Use the creating application identity for Keychain cleanup, just as users do.
    let output = tokio::process::Command::new(&context.environment.args.cli)
        .arg("--data-dir")
        .arg(&context.environment.state)
        .args(["server", "auth", "-n", "test", "--forget", "--json"])
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await?;
    if !output.status.success() {
        return Err("test Keychain cleanup failed; retain local test state for retry".into());
    }
    Ok(())
}

async fn exercise_saved(context: &Context, password: &Zeroizing<String>) -> ProbeResult<()> {
    eprintln!("saved password: validating initial password through a separate CLI");
    auth(context, password, true).await?;
    eprintln!("saved password: checking rejected rotation preserves original credential");
    let wrong = Zeroizing::new(uuid::Uuid::new_v4().to_string());
    auth(context, &wrong, false).await?;
    eprintln!("saved password: opening a new TUI using the stored credential");
    let config = context
        .environment
        .args
        .ssh_config
        .as_ref()
        .ok_or("trusted SSH config missing")?;
    let mut command = CommandBuilder::new(&context.environment.args.cli);
    command.arg("--data-dir");
    command.arg(&context.environment.state);
    command.arg("--ssh-config");
    command.arg(config);
    command.args(["-n", "test", "--path", &context.environment.workspace]);
    command.env_remove("REMOTE_CODEX_ACCEPTANCE_CONTROL");
    command.env("TERM", "xterm-256color");
    let mut terminal = Terminal::spawn(command)?;
    terminal.wait_for("/permissions").await?;
    tui::full_access(&terminal).await?;
    context.model.script([function(
        "exec_command",
        serde_json::json!({
            "cmd":"printf once >> saved-password-count; sleep 5; printf done > saved-password-done",
            "workdir":context.environment.workspace,"yield_time_ms":10000
        }),
        None,
    )])?;
    terminal
        .type_command("Run the fixed saved password recovery task.")
        .await?;
    tokio::time::timeout(Duration::from_secs(20), async {
        while context.environment.absent("saved-password-count").await? {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    ssh::interrupt_master(terminal.process_id()?).await?;
    terminal.wait_for(MARKER).await?;
    if context.environment.read("saved-password-count").await? != "once"
        || context.environment.read("saved-password-done").await? != "done"
    {
        return Err("saved-password recovery lost or repeated work".into());
    }
    terminal.type_command("/quit").await?;
    if terminal.wait_exit().await? != Some(0) {
        return Err("saved-password TUI exit failed".into());
    }
    let diagnostics = terminal.diagnostics();
    if diagnostics.contains(password.as_str())
        || diagnostics.contains("password:")
        || diagnostics.contains("SSH authentication is required")
    {
        return Err("saved-password connection exposed a password or required re-entry".into());
    }
    std::fs::write(
        context.environment.args.output.join("saved-password.txt"),
        diagnostics,
    )?;
    for entry in std::fs::read_dir(&context.environment.state)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let bytes = std::fs::read(entry.path())?;
            if bytes
                .windows(password.len())
                .any(|value| value == password.as_bytes())
            {
                return Err("SSH password was persisted in local state files".into());
            }
        }
    }
    Ok(())
}

async fn auth(context: &Context, password: &str, success: bool) -> ProbeResult<()> {
    let config = context
        .environment
        .args
        .ssh_config
        .as_ref()
        .ok_or("trusted SSH config missing")?;
    let mut child = tokio::process::Command::new(&context.environment.args.cli)
        .arg("--data-dir")
        .arg(&context.environment.state)
        .arg("--ssh-config")
        .arg(config)
        .args(["server", "auth", "-n", "test", "--password-stdin", "--json"])
        .env_remove("REMOTE_CODEX_ACCEPTANCE_CONTROL")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut input = child.stdin.take().ok_or("password pipe missing")?;
    input.write_all(password.as_bytes()).await?;
    input.shutdown().await?;
    drop(input);
    let output = tokio::time::timeout(Duration::from_secs(45), child.wait_with_output()).await??;
    if output
        .stdout
        .windows(password.len())
        .any(|value| value == password.as_bytes())
        || output
            .stderr
            .windows(password.len())
            .any(|value| value == password.as_bytes())
    {
        return Err("password leaked in authentication output".into());
    }
    if output.status.success() != success {
        return Err("saved password authentication outcome was unexpected".into());
    }
    Ok(())
}
