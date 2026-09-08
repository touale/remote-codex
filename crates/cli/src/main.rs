mod args;
mod chat;
mod chat_prepare;
mod config;
mod connection;
mod frontend;
mod output;
mod progress;
mod selection;
mod server;
mod ui;

use args::{Cli, Command};
use remote_codex_client::{
    ClientError, Result,
    store::{LocalStore, default_data_dir},
};
use std::{
    path::Path,
    process::{ExitCode, Stdio},
};

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse_argv(std::env::args_os().collect()).unwrap_or_else(|error| error.exit());
    match run(&cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if cli.json {
                let _ = output::json(
                    &serde_json::json!({"schema_version":2,"error":{"code":error.code(),"exit_code":error.exit_code(),"message":error.to_string(),"outcome_unknown":error.outcome_is_unknown()}}),
                );
            } else {
                eprintln!("remote-codex: {}", error);
            }
            ExitCode::from(error.exit_code())
        }
    }
}

async fn run(cli: &Cli) -> Result<()> {
    if let Some(Command::Codex(arguments)) = &cli.command {
        match arguments.first().and_then(|s| s.to_str()) {
            Some("connect" | "create") => {
                return Err(ClientError::Argument(
                    "connect/create were replaced by server add -n NAME --addr USER@HOST; start a session with remote-codex -n NAME",
                ));
            }
            Some("use") => {
                return Err(ClientError::Argument(
                    "use was replaced by remote-codex -n NAME --path /remote/project",
                ));
            }
            Some("status") => return Err(ClientError::Argument("use remote-codex server list")),
            Some("jobs") => {
                return Err(ClientError::Argument(
                    "jobs has been removed; continue your Codex session with remote-codex resume --all",
                ));
            }
            Some("login") => {
                return Err(ClientError::Argument(
                    "run codex login on this computer; remote environments share your local Codex account",
                ));
            }
            Some("update") => {
                return Err(ClientError::Argument(
                    "update local Codex through its installation method; compatible remote execution resources are prepared automatically when connecting",
                ));
            }
            Some(
                "exec" | "fork" | "review" | "logout" | "app-server" | "server" | "remove"
                | "disconnect",
            ) => {
                return Err(ClientError::Unsupported(
                    "this command has not been validated for remote execution",
                ));
            }
            _ => {}
        }
    }
    if matches!(
        &cli.command,
        Some(Command::Shell {
            legacy: Some(_),
            ..
        })
    ) {
        return Err(ClientError::Argument(
            "shell init is no longer needed; use remote-codex -n NAME, or remote-codex shell -n NAME for SSH",
        ));
    }
    if matches!(&cli.command, Some(Command::Config { .. })) {
        config::require_server(cli)?;
    }
    let directory = cli
        .data_dir
        .clone()
        .map(Ok)
        .unwrap_or_else(default_data_dir)?;
    let store = LocalStore::open(&directory).await?;
    let result = match &cli.command {
        Some(Command::Server { command }) => server::run(cli, &store, &directory, command).await,
        Some(Command::Config { command }) => config::run(cli, &store, command).await,
        Some(Command::Resume {
            id,
            archived,
            read,
            cursor,
            ..
        }) => {
            chat::resume(
                cli,
                &store,
                &directory,
                id.as_deref(),
                *archived,
                *read,
                cursor.as_deref(),
            )
            .await
        }
        Some(Command::Shell { .. }) => shell(cli, &store, &directory).await,
        Some(Command::Codex(arguments)) => chat::start(cli, &store, &directory, arguments).await,
        None => chat::start(cli, &store, &directory, &[]).await,
    };
    store.close().await;
    result
}

async fn shell(cli: &Cli, store: &LocalStore, directory: &Path) -> Result<()> {
    if cli.json || !ui::interactive() {
        return Err(ClientError::Argument(
            "shell requires an interactive terminal",
        ));
    }
    let record = selection::server(cli, store, directory).await?;
    let access = store.server_access(&record.id).await?;
    let ssh = remote_codex_client::ssh::SshTransport::connect_with(
        remote_codex_client::ssh::ConnectOptions {
            config: cli.ssh_config.clone().or(access.ssh_config),
            identity_file: access.identity_file,
            batch: false,
        },
        &record.endpoint,
    )
    .await?;
    let command = match &cli.path {
        Some(path) => {
            if !path.starts_with('/') || path.chars().any(char::is_control) {
                return Err(ClientError::Argument(
                    "shell --path requires a remote absolute directory",
                ));
            }
            Some(format!(
                "cd -- {} && exec \"${{SHELL:-/bin/sh}}\" -l",
                remote_codex_client::ssh::quote(path)?
            ))
        }
        None => None,
    };
    let status = ssh
        .command(&record.endpoint, command.as_deref(), true)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;
    ssh.close().await?;
    if !status.success() {
        return Err(ClientError::Ssh(status.code().unwrap_or(255)));
    }
    Ok(())
}
