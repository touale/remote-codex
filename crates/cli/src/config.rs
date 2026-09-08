use remote_codex_client::{
    ClientError, Result,
    config::{ConfigInput, ConfigKey},
    credentials::{NativeVault, set_secret},
    store::{ConnectionRecord, LocalStore},
};
use std::io::Read;
use zeroize::Zeroizing;

use crate::{
    args::{Cli, ConfigCommand},
    output,
};

pub(crate) async fn run(cli: &Cli, store: &LocalStore, command: &ConfigCommand) -> Result<()> {
    let record = store.find_connection(require_server(cli)?).await?;
    let server = &record.id;
    match command {
        ConfigCommand::List { overrides, .. } => output::config(
            &store.config_report(server, *overrides).await?,
            cli.json,
            false,
        ),
        ConfigCommand::Get { key, .. } => {
            let key = ConfigKey::parse(key)?.name();
            let mut report = store.config_report(server, false).await?;
            report.items.retain(|item| item.setting.key == key);
            if report.items.is_empty() {
                return Err(ClientError::NotFound);
            }
            output::config(&report, cli.json, true)
        }
        ConfigCommand::Set {
            key,
            value,
            stdin,
            secret,
            if_revision,
            ..
        } => {
            if key == "workspace" {
                return Err(ClientError::Argument(
                    "workspace is selected per session; use remote-codex -n NAME --path /remote/project",
                ));
            }
            let raw = if *stdin {
                read_input().await?
            } else {
                Zeroizing::new(
                    value
                        .clone()
                        .ok_or(ClientError::Argument("configuration value is required"))?,
                )
            };
            let expected =
                checked_revision(store.config_snapshot(server).await?.revision, *if_revision)?;
            if *secret {
                let vault = NativeVault::new(&store.installation_id().await?);
                set_secret(store, &vault, server, key, &raw, expected).await?;
            } else {
                store
                    .set_config(server, key, ConfigInput::Plain(&raw), expected)
                    .await?;
            }
            synchronize(cli, store, record.clone()).await;
            output::config(&store.config_report(server, false).await?, cli.json, false)
        }
        ConfigCommand::Unset {
            key, if_revision, ..
        } => {
            let expected =
                checked_revision(store.config_snapshot(server).await?.revision, *if_revision)?;
            store.unset_config(server, key, expected).await?;
            synchronize(cli, store, record.clone()).await;
            output::config(&store.config_report(server, false).await?, cli.json, false)
        }
    }
}

pub(crate) fn require_server(cli: &Cli) -> Result<&str> {
    cli.connection.as_deref().ok_or(ClientError::Argument(
        "specify a server with -n NAME; example: remote-codex config list -n dev",
    ))
}

async fn synchronize(cli: &Cli, store: &LocalStore, record: ConnectionRecord) {
    if let Ok(remote) = remote_codex_client::remote::Remote::connect_running(
        store,
        record,
        cli.ssh_config.clone(),
        true,
    )
    .await
    {
        let _ = remote.synchronize(store).await;
        let _ = remote.close().await;
    }
}

fn checked_revision(
    current: remote_codex_client::store::Revision,
    expected: Option<i64>,
) -> Result<remote_codex_client::store::Revision> {
    if expected.is_some_and(|revision| revision != current.saved) {
        return Err(ClientError::RevisionConflict);
    }
    Ok(current)
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
