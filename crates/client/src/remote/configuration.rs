use crate::{
    ClientError, Result,
    config::{ConfigKey, ConfigValue},
    credentials::{CredentialVault, NativeVault},
    store::{ConnectionRecord, LocalStore},
};
use remote_codex_protocol::RemoteConfig;
use std::collections::BTreeMap;

pub(super) async fn resolve(store: &LocalStore, record: &ConnectionRecord) -> Result<RemoteConfig> {
    let (snapshot, runtime) = store.sync_snapshot(&record.id).await?;
    let vault = NativeVault::new(&store.installation_id().await?);
    let mut values = BTreeMap::new();
    for (key, value) in snapshot.effective.entries() {
        if *key == ConfigKey::ReconnectMaxAttempts {
            continue;
        }
        let raw = match value {
            ConfigValue::Secret(reference) => vault.read(reference).await?.to_string(),
            ConfigValue::Text(value) => value.clone(),
            value => serde_json::to_string(&value.visible())?
                .trim_matches('"')
                .to_owned(),
        };
        values.insert(key.name(), raw);
    }
    Ok(RemoteConfig {
        codex: runtime
            .ok_or(ClientError::Argument("server runtime is not prepared"))?
            .executable
            .clone(),
        values,
        revision: snapshot.revision.saved,
    })
}
