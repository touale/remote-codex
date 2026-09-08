use crate::{
    args::{AddArgs, Cli},
    ui,
};
use remote_codex_client::{
    ClientError, Result,
    store::{ConnectionRecord, LocalStore},
};
use std::path::Path;

pub(crate) async fn server(
    cli: &Cli,
    store: &LocalStore,
    directory: &Path,
) -> Result<ConnectionRecord> {
    if let Some(name) = &cli.connection {
        return store.find_connection(name).await;
    }
    if !ui::interactive() || cli.json {
        return Err(ClientError::Argument(
            "specify the remote server with -n NAME",
        ));
    }
    let mut servers = store.list_connections().await?;
    if servers.is_empty() {
        crate::server::add(cli, store, directory, &AddArgs::default()).await?;
        servers = store.list_connections().await?;
    }
    if servers.len() == 1 {
        return servers.pop().ok_or(ClientError::NotFound);
    }
    let labels = servers
        .iter()
        .map(|r| format!("{}  {}", r.name, r.endpoint.default_name()))
        .collect::<Vec<_>>();
    let index = ui::choose("Select a remote server", &labels)?;
    Ok(servers.remove(index))
}

pub(crate) async fn workspace(
    cli: &Cli,
    store: &LocalStore,
    server: &ConnectionRecord,
) -> Result<String> {
    if let Some(path) = &cli.path {
        return Ok(path.clone());
    }
    if !ui::interactive() || cli.json {
        return Err(ClientError::Argument(
            "specify the remote workspace with --path",
        ));
    }
    let mut recent = store.workspaces(&server.id).await?;
    if !recent.is_empty() {
        recent.push("Enter another remote path…".into());
        let index = ui::choose(&format!("Workspace on {}", ui::text(&server.name)), &recent)?;
        if index + 1 < recent.len() {
            return Ok(recent.remove(index));
        }
    }
    ui::input("Remote project directory (absolute path)")
}
