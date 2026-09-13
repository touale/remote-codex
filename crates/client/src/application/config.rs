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
        let before = state.store.config_snapshot(&server.id).await?;
        let revision = before.revision;
        if expected.is_some_and(|expected| expected != revision.saved) {
            return Err(ClientError::RevisionConflict);
        }
        let result = if secret {
            let vault = NativeVault::new(&state.store.installation_id().await?);
            set_secret(&state.store, &vault, &server.id, key, value, revision).await
        } else {
            state
                .store
                .set_config(&server.id, key, ConfigInput::Plain(value), revision)
                .await
        };
        state.cleanup_credentials().await;
        result?;
        self.synchronize(server, &before.effective).await;
        self.list(name, false).await
    }

    /// Validate all plain settings before committing one optimistic transaction.
    pub async fn set_many(
        &self,
        name: &str,
        updates: &[(String, String)],
        expected: i64,
    ) -> Result<ConfigReport> {
        let state = &self.client.0;
        let server = state.store.find_connection(name).await?;
        let before = state.store.config_snapshot(&server.id).await?;
        let revision = before.revision;
        if revision.saved != expected {
            return Err(ClientError::RevisionConflict);
        }
        state
            .store
            .set_many_config(&server.id, updates, revision)
            .await?;
        self.synchronize(server, &before.effective).await;
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
        let before = state.store.config_snapshot(&server.id).await?;
        let revision = before.revision;
        if expected.is_some_and(|expected| expected != revision.saved) {
            return Err(ClientError::RevisionConflict);
        }
        let result = state.store.unset_config(&server.id, key, revision).await;
        state.cleanup_credentials().await;
        result?;
        self.synchronize(server, &before.effective).await;
        self.list(name, false).await
    }

    async fn synchronize(&self, server: crate::store::ConnectionRecord, before: &EffectiveConfig) {
        let state = &self.client.0;
        let Ok(after) = state.store.config_snapshot(&server.id).await else {
            return;
        };
        let changed = before
            .entries()
            .chain(after.effective.entries())
            .any(|(key, _)| {
                *key != ConfigKey::ReconnectMaxAttempts
                    && before.get(key) != after.effective.get(key)
            });
        if !changed {
            return;
        }
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
