use super::{Context, ssh, tui};
use portable_pty::CommandBuilder;
use remote_codex_test_support::{
    ProbeResult,
    model::{MARKER, function},
    terminal::Terminal,
};
use serde_json::json;
use std::{
    io::{BufRead, Write},
    time::Duration,
};
use zeroize::Zeroizing;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let password = read_password()?;
    let config = context.environment.args.ssh_config.as_ref().ok_or(
        "password test requires a trusted --ssh-config with password authentication enabled",
    )?;
    let mut command = CommandBuilder::new(&context.environment.args.cli);
    command.arg("--data-dir");
    command.arg(&context.environment.state);
    command.arg("--ssh-config");
    command.arg(config);
    command.args(["-n", "test", "--path", &context.environment.workspace]);
    command.env_remove("REMOTE_CODEX_ACCEPTANCE_CONTROL");
    command.env("TERM", "xterm-256color");
    let mut terminal = Terminal::spawn(command)?;
    wait_password(&terminal, 1).await?;
    terminal.send(password.as_bytes())?;
    terminal.send(b"\r")?;
    terminal.wait_for("gpt-6-astra").await?;
    tui::full_access(&terminal).await?;
    context.model.script([function("exec_command", json!({"cmd":"printf once >> password-count; sleep 5; printf done > password-done", "workdir":context.environment.workspace,"yield_time_ms":10000}), None)])?;
    terminal
        .type_command("Run the fixed password recovery task.")
        .await?;
    tokio::time::timeout(Duration::from_secs(20), async {
        while context.environment.absent("password-count").await? {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    ssh::interrupt_master(terminal.process_id()?).await?;
    terminal
        .wait_for("SSH authentication is required to restore this session.")
        .await?;
    wait_password(&terminal, 2).await?;
    terminal.send(password.as_bytes())?;
    terminal.send(b"\r")?;
    terminal.wait_for(MARKER).await?;
    if context.environment.read("password-count").await? != "once"
        || context.environment.read("password-done").await? != "done"
    {
        return Err("password reconnection lost or replayed work".into());
    }
    terminal.type_command("/quit").await?;
    let status = terminal.wait_exit().await?;
    let diagnostics = terminal.diagnostics();
    if diagnostics.contains(password.as_str()) {
        return Err("password was echoed by the terminal".into());
    }
    std::fs::write(
        context
            .environment
            .args
            .output
            .join("password-recovery.txt"),
        &diagnostics,
    )?;
    if status != Some(0) {
        return Err("password recovery frontend did not exit cleanly".into());
    }
    eprintln!(
        "password recovery: real OpenSSH master terminated, password reauthentication restored the existing TUI and command exactly once"
    );
    super::saved_password::exercise(context, &password).await
}

async fn wait_password(terminal: &Terminal, count: usize) -> ProbeResult<()> {
    tokio::time::timeout(Duration::from_secs(45), async {
        while terminal.diagnostics().matches("password:").count() < count {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "SSH password prompt did not appear")?;
    Ok(())
}

pub(super) fn read_password() -> ProbeResult<Zeroizing<String>> {
    use nix::sys::termios::{self, LocalFlags, SetArg};
    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")?;
    let saved = termios::tcgetattr(&tty)?;
    let mut hidden = saved.clone();
    hidden.local_flags.remove(LocalFlags::ECHO);
    termios::tcsetattr(&tty, SetArg::TCSANOW, &hidden)?;
    let mut password = Zeroizing::new(String::new());
    let read = (|| -> std::io::Result<()> {
        write!(
            tty,
            "Acceptance SSH password (temporary Keychain entry will be removed): "
        )?;
        tty.flush()?;
        std::io::BufReader::new(tty.try_clone()?).read_line(&mut password)?;
        Ok(())
    })();
    termios::tcsetattr(&tty, SetArg::TCSANOW, &saved)?;
    writeln!(tty)?;
    read?;
    while password.ends_with(['\r', '\n']) {
        password.pop();
    }
    if password.is_empty() {
        return Err("test password is empty".into());
    }
    Ok(password)
}
