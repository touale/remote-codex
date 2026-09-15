mod args;
mod chat;
mod config;
mod context;
mod frontend;
mod output;
mod progress;
mod selection;
mod server;
mod ui;
mod update;

use args::{Cli, Command};
use context::Context;
use remote_codex_client::{ClientError, Result};
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args_os().collect();
    let json = arguments
        .iter()
        .take_while(|arg| *arg != "--")
        .any(|arg| arg == "--json");
    let cli = match Cli::parse_argv(arguments) {
        Ok(cli) => cli,
        Err(error) if !error.use_stderr() => error.exit(),
        Err(error) => {
            if json {
                let _ = output::failure("INVALID_ARGUMENT", &error.to_string(), false, false);
            } else {
                let _ = error.print();
            }
            return ExitCode::from(2);
        }
    };
    match run(&cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if cli.json {
                let _ = output::error(&error);
            } else {
                eprintln!("remote-codex: {error}");
            }
            ExitCode::from(error.exit_code())
        }
    }
}

async fn run(cli: &Cli) -> Result<()> {
    if let Some(Command::Update { command }) = &cli.command {
        return update::run(cli, command.as_ref()).await;
    }
    if matches!(&cli.command, Some(Command::Config { .. })) {
        config::require_server(cli)?;
    }
    let context = Context::open(cli).await?;
    let background = if !cli.json
        && matches!(
            &cli.command,
            None | Some(Command::Resume { read: false, .. })
        ) {
        update::notice(cli.data_dir.clone()).await;
        Some(update::background(cli.data_dir.clone()))
    } else {
        None
    };
    let result = match &cli.command {
        Some(Command::Update { command }) => update::run(cli, command.as_ref()).await,
        Some(Command::Server { command }) => server::run(cli, &context, command).await,
        Some(Command::Config { command }) => config::run(cli, &context.client, command).await,
        Some(Command::Resume {
            id,
            archived,
            read,
            cursor,
            ..
        }) => {
            chat::resume(
                cli,
                &context,
                id.as_deref(),
                *archived,
                *read,
                cursor.as_deref(),
            )
            .await
        }
        Some(Command::Shell) => {
            if cli.json || !ui::interactive() {
                Err(ClientError::Argument(
                    "shell requires an interactive terminal",
                ))
            } else {
                let server = selection::server(cli, &context).await?;
                context
                    .client
                    .servers()
                    .shell(&server.name, cli.path.as_deref())
                    .await
            }
        }
        None => chat::start(cli, &context).await,
    };
    context.client.close().await;
    if let Some(background) = background {
        background.abort();
        let _ = background.await;
        update::notice(cli.data_dir.clone()).await;
    }
    result
}
