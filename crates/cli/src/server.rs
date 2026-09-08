mod wizard;
use crate::{
    args::{AddArgs, Cli, ServerCommand},
    context::Context,
    output, ui,
};
use remote_codex_client::{Result, application::AddServer};

pub(crate) async fn run(cli: &Cli, context: &Context, command: &ServerCommand) -> Result<()> {
    match command {
        ServerCommand::Add(args) => add(cli, context, args).await,
        ServerCommand::List => {
            let list = context
                .client
                .servers()
                .list(cli.connection.as_deref())
                .await?;
            if cli.json {
                output::json(&list)
            } else {
                output::server_list(&list.servers, &list.refresh)
            }
        }
        ServerCommand::Remove {
            name,
            revoke_key,
            yes,
        } => {
            context.client.servers().find(name).await?;
            if !yes
                && !ui::confirm(
                    "Remove this server and its local session bindings? Local Codex history and remote files and tasks remain.",
                    false,
                )?
            {
                return Ok(());
            }
            context.client.servers().remove(name, *revoke_key).await?;
            if cli.json {
                output::json(&serde_json::json!({"removed":name,"remote_data_preserved":true}))
            } else {
                println!(
                    "Removed local server {}. Local Codex history and remote files and tasks were preserved.",
                    ui::text(name)
                );
                Ok(())
            }
        }
    }
}

pub(crate) async fn add(cli: &Cli, context: &Context, args: &AddArgs) -> Result<()> {
    let settings = wizard::collect(cli, &context.client, args).await?;
    let server = context
        .prepare(context.client.servers().add(AddServer {
            name: settings.name,
            address: settings.address,
            port: settings.port,
            settings: settings.changes,
            identity: args.identity.clone(),
            install_key: settings.install_key,
        }))
        .await?;
    if cli.json {
        output::json(&serde_json::json!({"server":server}))
    } else {
        println!(
            "Environment {} is ready. Codex and login stay on this computer.",
            ui::text(&server.name)
        );
        println!(
            "Start a workspace: remote-codex -n {}",
            ui::text(&server.name)
        );
        Ok(())
    }
}
