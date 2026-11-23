#[cfg(unix)]
use remote_codex_client::application::TerminalAttachment;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
use crate::{args::Cli, ui};
use remote_codex_client::{ClientError, Result, application::SessionHandle};
use std::ffi::OsString;
#[cfg(unix)]
use std::process::Stdio;

pub(crate) async fn run(cli: &Cli, runtime: SessionHandle, arguments: &[OsString]) -> Result<()> {
    #[cfg(unix)]
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
            #[cfg(unix)]
            let frontend = unix::attach(&mut gateway, &mut interrupt).await;
            #[cfg(windows)]
            let frontend = windows::attach(&mut gateway).await;
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
        return Ok(value.into());
    }
    #[cfg(windows)]
    {
        Ok(format!("'{}'", value.replace('\'', "''")))
    }
    #[cfg(unix)]
    {
        shlex::try_quote(value)
            .map(|word| word.into_owned())
            .map_err(|_| ClientError::Argument("resume argument contains a null byte"))
    }
}
#[cfg(all(test, unix))]
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
