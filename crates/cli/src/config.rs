use crate::{
    args::{Cli, ConfigCommand},
    output,
};
use remote_codex_client::{ClientError, Result, application::Client};
use std::io::Read;
use zeroize::Zeroizing;

pub(crate) async fn run(cli: &Cli, client: &Client, command: &ConfigCommand) -> Result<()> {
    let name = require_server(cli)?;
    let service = client.config();
    let (report, get) = match command {
        ConfigCommand::List { overrides } => (service.list(name, *overrides).await?, false),
        ConfigCommand::Get { key } => (service.get(name, key).await?, true),
        ConfigCommand::Set {
            key,
            value,
            stdin,
            secret,
            if_revision,
        } => {
            let raw = if *stdin {
                read_input().await?
            } else {
                Zeroizing::new(
                    value
                        .clone()
                        .ok_or(ClientError::Argument("configuration value is required"))?,
                )
            };
            (
                service.set(name, key, &raw, *secret, *if_revision).await?,
                false,
            )
        }
        ConfigCommand::Unset { key, if_revision } => {
            (service.unset(name, key, *if_revision).await?, false)
        }
    };
    output::config(&report, cli.json, get)
}

pub(crate) fn require_server(cli: &Cli) -> Result<&str> {
    cli.connection.as_deref().ok_or(ClientError::Argument(
        "specify a server with -n NAME; example: remote-codex config list -n dev",
    ))
}

async fn read_input() -> Result<Zeroizing<String>> {
    tokio::task::spawn_blocking(|| {
        let mut value = Zeroizing::new(String::new());
        std::io::stdin()
            .take(32 * 1024 + 3)
            .read_to_string(&mut value)?;
        if value.ends_with('\n') {
            value.pop();
            if value.ends_with('\r') {
                value.pop();
            }
        }
        if value.len() > 32 * 1024 {
            return Err(ClientError::Argument("configuration value exceeds 32 KiB"));
        }
        Ok(value)
    })
    .await
    .map_err(|_| ClientError::Argument("failed to read configuration input"))?
}
