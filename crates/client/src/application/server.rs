use super::{Client, ServerStatus};
use crate::{
    ClientError, Result,
    connection::SshEndpoint,
    remote::Remote,
    servers,
    ssh::{ConnectOptions, SshTransport},
};
use serde::Serialize;
use std::{path::PathBuf, process::Stdio};

#[derive(Clone, Debug, Serialize)]
pub struct ServerSummary {
    pub id: String,
    pub name: String,
    pub endpoint: SshEndpoint,
}

#[derive(Debug, Serialize)]
pub struct ServerList {
    pub servers: Vec<ServerSummary>,
    pub refresh: Vec<ServerStatus>,
}

pub struct AddServer {
    pub name: String,
    pub address: String,
    pub port: Option<u16>,
    pub settings: Vec<(String, String)>,
    pub identity: Option<PathBuf>,
    pub install_key: bool,
    /// Save this password in the local OS vault after verifying it with SSH.
    pub password: Option<zeroize::Zeroizing<String>>,
}

pub struct ServerService {
    pub(super) client: Client,
}

impl ServerService {
    pub async fn terminal(
        &self,
        name: &str,
        columns: u16,
        rows: u16,
    ) -> Result<super::ShellHandle> {
        let state = &self.client.0;
        state.ensure_open()?;
        let record = state.store.find_connection(name).await?;
        let lock = crate::workspace_lock::WorkspaceLock::acquire(
            &state.directory,
            &record.id,
            None,
            false,
        )?;
        let remote = state.connect(record).await?;
        let path = remote.identity.home.clone();
        if !std::path::Path::new(&path).is_absolute() {
            return Err(ClientError::Argument(
                "Remote home directory is unavailable.",
            ));
        }
        super::shell::open(remote, &path, columns, rows, lock).await
    }

    pub async fn saved(&self) -> Result<Vec<ServerSummary>> {
        Ok(self
            .client
            .0
            .store
            .list_connections()
            .await?
            .into_iter()
            .map(summary)
            .collect())
    }

    pub async fn find(&self, name: &str) -> Result<ServerSummary> {
        Ok(summary(self.client.0.store.find_connection(name).await?))
    }

    pub async fn list(&self, selected: Option<&str>) -> Result<ServerList> {
        let state = &self.client.0;
        let refresh =
            servers::status::refresh(&state.store, selected, state.options.ssh_config.clone())
                .await?;
        let servers = match selected {
            Some(name) => vec![self.find(name).await?],
            None => self.saved().await?,
        };
        Ok(ServerList { servers, refresh })
    }

    pub async fn add(&self, request: AddServer) -> Result<ServerSummary> {
        let state = &self.client.0;
        SshEndpoint::parse(&request.address, request.port)?;
        if request.install_key && !state.options.interactive {
            return Err(ClientError::Argument(
                "install-key requires an interactive terminal; supply an existing identity for unattended initialization",
            ));
        }
        if state.options.service.bytes.is_empty() {
            return Err(ClientError::Argument(
                "service bundle missing; follow Build from source in CONTRIBUTING.md",
            ));
        }
        let program = state.program().await?;
        let native = remote_codex_adapter::program::inspect(&program).await?;
        let prepared = crate::ssh::interaction::scope(
            state.options.authentication.clone(),
            servers::add(
                &state.store,
                &state.directory,
                servers::AddServer {
                    name: &request.name,
                    codex_version: &native.version,
                    address: &request.address,
                    port: request.port,
                    settings: &request.settings,
                    identity: request.identity,
                    ssh_config: state.options.ssh_config.clone(),
                    install_key: request.install_key,
                    interactive: state.options.interactive,
                    password: request.password,
                },
                state.bundle(),
                |event| state.progress(event),
            ),
        )
        .await;
        state.cleanup_credentials().await;
        let remote = prepared?;
        state.progress(crate::progress::PrepareEvent::Stage(
            crate::progress::PrepareStage::Synchronize,
        ));
        let synchronized = remote.synchronize(&state.store).await;
        let record = summary(remote.server.clone());
        let closed = remote.close().await;
        synchronized?;
        closed?;
        Ok(record)
    }

    pub async fn remove(&self, name: &str, revoke_key: bool) -> Result<()> {
        let state = &self.client.0;
        let record = state.store.find_connection(name).await?;
        let _lock = crate::workspace_lock::WorkspaceLock::acquire(
            &state.directory,
            &record.id,
            None,
            true,
        )?;
        if revoke_key {
            let remote = Remote::connect_running(
                &state.store,
                record.clone(),
                state.options.ssh_config.clone(),
                !state.options.interactive,
            )
            .await?;
            let result = servers::keys::revoke(&remote.ssh, &record.endpoint, &remote.access).await;
            remote.close().await?;
            result?;
        }
        state.store.remove_server(&record.id).await?;
        state.cleanup_credentials().await;
        Ok(())
    }

    pub async fn workspaces(&self, name: &str) -> Result<Vec<String>> {
        let record = self.client.0.store.find_connection(name).await?;
        self.client.0.store.workspaces(&record.id).await
    }

    pub async fn shell(&self, name: &str, path: Option<&str>) -> Result<()> {
        let state = &self.client.0;
        if !state.options.interactive {
            return Err(ClientError::Argument(
                "shell requires an interactive terminal",
            ));
        }
        let command = match path {
            Some(path) => {
                if !path.starts_with('/') || path.chars().any(char::is_control) {
                    return Err(ClientError::Argument(
                        "shell --path requires a remote absolute directory",
                    ));
                }
                Some(format!(
                    "cd -- {} && exec \"${{SHELL:-/bin/sh}}\" -l",
                    crate::ssh::quote(path)?
                ))
            }
            None => None,
        };
        let record = state.store.find_connection(name).await?;
        let access = state.store.server_access(&record.id).await?;
        let ssh = SshTransport::connect_with(
            ConnectOptions {
                interaction: crate::ssh::interaction::current(),
                config: state.options.ssh_config.clone().or(access.ssh_config),
                identity_file: access.identity_file,
                batch: false,
                credentials: Some(crate::ssh::credentials::Credentials::saved(
                    &state.store,
                    &record.id,
                )),
            },
            &record.endpoint,
        )
        .await?;
        let result = ssh
            .command(&record.endpoint, command.as_deref(), true)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await;
        ssh.close().await?;
        let status = result?;
        if !status.success() {
            return Err(ClientError::Ssh(status.code().unwrap_or(255)));
        }
        Ok(())
    }
}

fn summary(record: crate::store::ConnectionRecord) -> ServerSummary {
    ServerSummary {
        id: record.id,
        name: record.name,
        endpoint: record.endpoint,
    }
}
