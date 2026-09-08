mod wizard;
use crate::{
    args::{AddArgs, Cli, ServerCommand},
    output,
    progress::PrepareProgress,
    ui,
};
use remote_codex_client::{
    ClientError, Result,
    remote::Remote,
    servers::{self, AddServer, ServerBundle},
    store::LocalStore,
};
use std::{
    cell::RefCell,
    path::Path,
    time::{Duration, Instant},
};

use crate::connection::bundled;

pub(crate) async fn run(
    cli: &Cli,
    store: &LocalStore,
    directory: &Path,
    command: &ServerCommand,
) -> Result<()> {
    match command {
        ServerCommand::Add(args) => add(cli, store, directory, args).await,
        ServerCommand::List => {
            let refreshed =
                servers::status::refresh(store, cli.connection.as_deref(), cli.ssh_config.clone())
                    .await?;
            let records = match &cli.connection {
                Some(name) => vec![store.find_connection(name).await?],
                None => store.list_connections().await?,
            };
            if cli.json {
                let mut result = Vec::new();
                for record in records {
                    let access = store.server_access(&record.id).await?;
                    result.push(serde_json::json!({"server":record,"access":access}));
                }
                output::json(
                    &serde_json::json!({"schema_version":2,"servers":result,"refresh":refreshed}),
                )?;
            } else {
                output::server_list(&records, &refreshed)?;
            }
            Ok(())
        }
        ServerCommand::Remove {
            name,
            revoke_key,
            yes,
        } => {
            let record = store.find_connection(name).await?;
            if !yes
                && !ui::confirm(
                    "Remove this server and its local session bindings? Local Codex history and remote files and tasks remain.",
                    false,
                )?
            {
                return Ok(());
            }
            if *revoke_key {
                let remote = Remote::connect_running(
                    store,
                    record.clone(),
                    cli.ssh_config.clone(),
                    !ui::interactive(),
                )
                .await?;
                servers::keys::revoke(&remote.ssh, &record.endpoint, &remote.access).await?;
                remote.close().await?;
            }
            store.remove_server(&record.id).await?;
            if cli.json {
                output::json(
                    &serde_json::json!({"schema_version":2,"removed":name,"remote_data_preserved":true}),
                )?;
            } else {
                println!(
                    "Removed local server {}. Local Codex history and remote files and tasks were preserved.",
                    ui::text(name)
                );
            }
            Ok(())
        }
    }
}

pub(crate) async fn add(
    cli: &Cli,
    store: &LocalStore,
    directory: &Path,
    args: &AddArgs,
) -> Result<()> {
    let settings = wizard::collect(cli, store, args).await?;
    // Validate before attempting network access or reporting a distribution issue.
    remote_codex_client::connection::SshEndpoint::parse(&settings.address, settings.port)?;
    if bundled::SERVICE.is_empty() {
        return Err(ClientError::Argument(
            "service bundle missing; build this checkout with python3 tools/package.py",
        ));
    }
    let display = RefCell::new(PrepareProgress::stderr(cli.json));
    let operation = async {
        let remote = servers::add(
            store,
            directory,
            AddServer {
                name: &settings.name,
                address: &settings.address,
                port: settings.port,
                settings: &settings.changes,
                identity: args.identity.clone(),
                ssh_config: cli.ssh_config.clone(),
                install_key: settings.install_key,
                interactive: settings.interactive,
            },
            ServerBundle {
                bytes: bundled::SERVICE,
                sha256: bundled::SERVICE_SHA,
            },
            |e| display.borrow_mut().event(e, Instant::now()),
        )
        .await?;
        Ok::<_, ClientError>(remote)
    };
    tokio::pin!(operation);
    let mut timer = tokio::time::interval(Duration::from_millis(200));
    let result = loop {
        tokio::select! {r=&mut operation=>break r,_=timer.tick()=>display.borrow_mut().tick(Instant::now())}
    };
    display.borrow_mut().finish(Instant::now());
    let remote = result?;
    if cli.json {
        output::json(
            &serde_json::json!({"schema_version":3,"server":store.find_connection(&settings.name).await?,"identity":remote.identity,"role":"execution_environment"}),
        )?;
    } else {
        println!(
            "Environment {} is ready. Codex and login stay on this computer.",
            ui::text(&settings.name)
        );
        println!(
            "Start a workspace: remote-codex -n {}",
            ui::text(&settings.name)
        );
    }
    remote.close().await
}
