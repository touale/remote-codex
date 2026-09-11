use super::ServerService;
use crate::{
    Result, servers,
    ssh::{ConnectOptions, credentials::Credentials},
};
use std::path::PathBuf;
use zeroize::Zeroizing;

#[derive(serde::Serialize)]
pub struct ServerAuthentication {
    pub identity: Option<PathBuf>,
    pub password_saved: bool,
}

impl ServerService {
    pub async fn authentication(&self, name: &str) -> Result<ServerAuthentication> {
        let store = &self.client.0.store;
        let record = store.find_connection(name).await?;
        Ok(ServerAuthentication {
            identity: store.server_access(&record.id).await?.identity_file,
            password_saved: store.ssh_credential(&record.id).await?.is_some(),
        })
    }
    /// Verify new SSH access before replacing the selected identity. Existing
    /// connections keep their authenticated transport until reopened.
    pub async fn save_identity(&self, name: &str, identity: Option<PathBuf>) -> Result<()> {
        let state = &self.client.0;
        let record = state.store.find_connection(name).await?;
        let access = state.store.server_access(&record.id).await?;
        let identity = identity.map(|path| path.canonicalize()).transpose()?;
        if identity.as_ref().is_some_and(|path| !path.is_file()) {
            return Err(crate::ClientError::Argument(
                "Choose a regular SSH private key file.",
            ));
        }
        let ssh = crate::ssh::SshTransport::connect_with(
            ConnectOptions {
                interaction: state.options.authentication.clone(),
                config: state.options.ssh_config.clone().or(access.ssh_config),
                identity_file: identity.clone(),
                batch: !state.options.interactive,
                credentials: Some(Credentials::saved(&state.store, &record.id)),
            },
            &record.endpoint,
        )
        .await?;
        let result = state
            .store
            .replace_identity(
                &record.id,
                access.identity_file.as_deref(),
                identity.as_deref(),
            )
            .await;
        ssh.close().await?;
        result
    }
    /// Validate and save a password without reinstalling the execution environment.
    pub async fn save_password(&self, name: &str, password: Zeroizing<String>) -> Result<()> {
        let state = &self.client.0;
        let record = state.store.find_connection(name).await?;
        let access = state.store.server_access(&record.id).await?;
        state.progress(crate::progress::PrepareEvent::Stage(
            crate::progress::PrepareStage::ConnectSsh,
        ));
        let result = servers::authentication::connect(
            &state.store,
            &record,
            ConnectOptions {
                interaction: state.options.authentication.clone(),
                config: state.options.ssh_config.clone().or(access.ssh_config),
                identity_file: access.identity_file,
                batch: !state.options.interactive,
                credentials: Some(Credentials::saved(&state.store, &record.id)),
            },
            Some(password),
        )
        .await;
        state.cleanup_credentials().await;
        result?.close().await
    }

    /// Forget the stored password locally; existing authenticated connections remain open.
    pub async fn forget_password(&self, name: &str) -> Result<()> {
        let state = &self.client.0;
        let record = state.store.find_connection(name).await?;
        state.store.forget_ssh_credentials(&record.id).await?;
        state.cleanup_credentials().await;
        Ok(())
    }
}
