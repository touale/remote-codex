mod authentication;
use crate::{args::Cli, ui};
use remote_codex_client::{
    ClientError, Result,
    application::{SessionHandle, TerminalAttachment},
};
use std::{ffi::OsString, process::Stdio};

pub(crate) async fn run(cli: &Cli, runtime: SessionHandle, arguments: &[OsString]) -> Result<()> {
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    println!(
        "Workspace: {} · {}",
        ui::text(runtime.server()),
        ui::text(&runtime.session().cwd)
    );
    println!("Local session: {}", ui::text(&runtime.session().id));
    let (runtime, result) = match runtime.terminal(arguments).await {
        Ok(mut gateway) => {
            let current = gateway.current.clone();
            let frontend = attach(&mut gateway, &mut interrupt).await;
            // Always collect the gateway's cause, even when its closure made the
            // native child exit with a generic nonzero status.
            let result = gateway.finish().await.and(frontend);
            (current.borrow().clone(), result)
        }
        Err(error) => (runtime, Err(error)),
    };
    // Every returning exit path shares cleanup and recovery guidance, including
    // failed frontend launch, native errors and interactive interruption.
    runtime.close().await;
    if let Some(id) = runtime.persisted_id() {
        match resume_command(cli, id) {
            Ok(command) => eprintln!("Resume: {command}"),
            Err(_) => eprintln!(
                "Resume session {} using the same local data directory.",
                ui::text(id)
            ),
        }
    }
    result
}

async fn attach(
    gateway: &mut TerminalAttachment,
    interrupt: &mut tokio::signal::unix::Signal,
) -> Result<()> {
    let terminal = authentication::Terminal::capture();
    let mut sessions = gateway.current.clone();
    let mut runtime = sessions.borrow_and_update().clone();
    let mut environment = runtime.environment();
    let mut auth_attempted = false;
    let mut closed = gateway.closed.clone();
    let mut child = gateway
        .command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let status = loop {
        tokio::select! {
          changed=sessions.changed()=>{
              if changed.is_ok() {
                  runtime = sessions.borrow_and_update().clone();
                  environment = runtime.environment();
                  auth_attempted = false;
              }
          },
          result=child.wait()=>break result?,
          _=closed.changed()=>break wait_for_exit(&mut child).await?,
          _=interrupt.recv()=>{
              if let Some(pid) = child.id().and_then(|pid| i32::try_from(pid).ok()) {
                  let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), nix::sys::signal::Signal::SIGINT);
              }
              break wait_for_exit(&mut child).await?;
          },
          changed=environment.changed()=>{
              if changed.is_err() { continue; }
              let state = environment.borrow_and_update().clone();
              if matches!(&state, remote_codex_client::session::EnvironmentState::Ready) { auth_attempted = false; }
              if matches!(&state, remote_codex_client::session::EnvironmentState::ActionRequired { code, .. } if code == "SSH_AUTH_REQUIRED") && !auth_attempted {
                  auth_attempted = true;
                  if let Some(pid) = child.id() && let Some(_suspended) = terminal.suspend(pid)? {
                      eprintln!("\nSSH authentication is required to restore this session.");
                      let result = tokio::select! {
                          result=runtime.authenticate()=>result,
                          _=interrupt.recv()=>Err(ClientError::Argument("authentication cancelled")),
                      };
                      if let Err(error) = result { eprintln!("{error}. Exit and resume this session to try again."); }
                  }
              }
          },
        }
    };
    if !status.success() {
        return Err(ClientError::CodexExit(
            u8::try_from(status.code().unwrap_or(1)).unwrap_or(1),
        ));
    }
    Ok(())
}

async fn wait_for_exit(
    child: &mut tokio::process::Child,
) -> std::io::Result<std::process::ExitStatus> {
    // Closing the frontend socket can precede its normal process exit. Allow
    // terminal cleanup to finish before killing an unresponsive frontend.
    match tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await {
        Ok(status) => status,
        Err(_) => {
            child.kill().await?;
            child.wait().await
        }
    }
}

fn resume_command(cli: &Cli, id: &str) -> Result<String> {
    let mut command = format!("remote-codex resume {}", word(id)?);
    for (flag, path) in [
        ("--data-dir", &cli.data_dir),
        ("--ssh-config", &cli.ssh_config),
    ] {
        if let Some(path) = path {
            let path = std::path::absolute(path)?;
            let path = path
                .to_str()
                .ok_or(ClientError::Argument("resume path is not UTF-8"))?;
            command.push_str(&format!(" {flag} {}", word(path)?));
        }
    }
    Ok(command)
}

fn word(value: &str) -> Result<String> {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_./:-".contains(&byte))
    {
        Ok(value.into())
    } else {
        shlex::try_quote(value)
            .map(|word| word.into_owned())
            .map_err(|_| ClientError::Argument("resume argument contains a null byte"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_command_preserves_explicit_paths_and_quotes_shell_characters() -> Result<()> {
        let cli = Cli::parse_argv(
            [
                "remote-codex",
                "--data-dir",
                "/tmp/state with spaces",
                "--ssh-config",
                "/tmp/it's ssh.conf",
            ]
            .into_iter()
            .map(Into::into)
            .collect(),
        )
        .map_err(|_| ClientError::Argument("test arguments"))?;
        let command = resume_command(&cli, "thread-id")?;
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("set -- {command}; printf '%s\\n' \"$@\""))
            .output()?;
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .collect::<Vec<_>>(),
            [
                "remote-codex",
                "resume",
                "thread-id",
                "--data-dir",
                "/tmp/state with spaces",
                "--ssh-config",
                "/tmp/it's ssh.conf"
            ]
        );
        Ok(())
    }
}
