use super::Client;
use crate::{
    ClientError, Result,
    config::{ConfigInput, ConfigKey, ConfigReport, EffectiveConfig},
    credentials::{NativeVault, set_secret},
};

pub struct ConfigService {
    pub(super) client: Client,
}

impl ConfigService {
    pub async fn list(&self, name: &str, overrides: bool) -> Result<ConfigReport> {
        let state = &self.client.0;
        let server = state.store.find_connection(name).await?;
        state.store.config_report(&server.id, overrides).await
    }

    pub async fn get(&self, name: &str, key: &str) -> Result<ConfigReport> {
        let key = ConfigKey::parse(key)?.name();
        let mut report = self.list(name, false).await?;
        report.items.retain(|item| item.setting.key == key);
        if report.items.is_empty() {
            return Err(ClientError::NotFound);
        }
        Ok(report)
    }

    pub async fn effective(&self, name: &str) -> Result<EffectiveConfig> {
        let state = &self.client.0;
        let server = state.store.find_connection(name).await?;
        Ok(state.store.config_snapshot(&server.id).await?.effective)
    }

    pub async fn set(
        &self,
        name: &str,
        key: &str,
        value: &str,
        secret: bool,
        expected: Option<i64>,
    ) -> Result<ConfigReport> {
        let state = &self.client.0;
        let server = state.store.find_connection(name).await?;
        let revision = state.store.config_snapshot(&server.id).await?.revision;
        if expected.is_some_and(|expected| expected != revision.saved) {
            return Err(ClientError::RevisionConflict);
        }
        if secret {
            let vault = NativeVault::new(&state.store.installation_id().await?);
            set_secret(&state.store, &vault, &server.id, key, value, revision).await?;
        } else {
            state
                .store
                .set_config(&server.id, key, ConfigInput::Plain(value), revision)
                .await?;
        }
        self.synchronize(server).await;
        self.list(name, false).await
    }

    pub async fn unset(
        &self,
        name: &str,
        key: &str,
        expected: Option<i64>,
    ) -> Result<ConfigReport> {
        let state = &self.client.0;
        let server = state.store.find_connection(name).await?;
        let revision = state.store.config_snapshot(&server.id).await?.revision;
        if expected.is_some_and(|expected| expected != revision.saved) {
            return Err(ClientError::RevisionConflict);
        }
        state.store.unset_config(&server.id, key, revision).await?;
        self.synchronize(server).await;
        self.list(name, false).await
    }

    async fn synchronize(&self, server: crate::store::ConnectionRecord) {
        let state = &self.client.0;
        if let Ok(remote) = crate::remote::Remote::connect_running(
            &state.store,
            server,
            state.options.ssh_config.clone(),
            true,
        )
        .await
        {
            let _ = remote.synchronize(&state.store).await;
            let _ = remote.close().await;
        }
    }
}
