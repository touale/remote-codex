use super::ServerService;
use crate::{
    Result, servers,
    ssh::{ConnectOptions, credentials::Credentials},
};
use zeroize::Zeroizing;

impl ServerService {
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
