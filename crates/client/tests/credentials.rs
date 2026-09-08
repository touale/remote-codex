use remote_codex_client::{
    ClientError, Result,
    config::{ConfigKey, ConfigValue, SecretRef},
    connection::SshEndpoint,
    credentials::{CredentialVault, set_secret},
    store::LocalStore,
};
use std::{collections::HashMap, sync::Mutex};
use zeroize::Zeroizing;

#[derive(Default)]
struct MemoryVault {
    values: Mutex<HashMap<String, String>>,
}

impl CredentialVault for MemoryVault {
    async fn put(&self, reference: &SecretRef, value: &str) -> Result<()> {
        self.values
            .lock()
            .map_err(|_| ClientError::Credentials)?
            .insert(reference.id().to_owned(), value.to_owned());
        Ok(())
    }
    async fn read(&self, reference: &SecretRef) -> Result<Zeroizing<String>> {
        self.values
            .lock()
            .map_err(|_| ClientError::Credentials)?
            .get(reference.id())
            .cloned()
            .map(Zeroizing::new)
            .ok_or(ClientError::Credentials)
    }
    async fn delete(&self, reference: &SecretRef) -> Result<()> {
        self.values
            .lock()
            .map_err(|_| ClientError::Credentials)?
            .remove(reference.id());
        Ok(())
    }
}

struct RacingVault {
    inner: MemoryVault,
    store: LocalStore,
    server: String,
}

impl CredentialVault for RacingVault {
    async fn put(&self, reference: &SecretRef, value: &str) -> Result<()> {
        self.inner.put(reference, value).await?;
        let server = &self.server;
        let revision = self.store.config_snapshot(server).await?.revision;
        self.store
            .set_config(
                server,
                "background",
                remote_codex_client::config::ConfigInput::Plain("false"),
                revision,
            )
            .await?;
        Ok(())
    }
    async fn read(&self, reference: &SecretRef) -> Result<Zeroizing<String>> {
        self.inner.read(reference).await
    }
    async fn delete(&self, reference: &SecretRef) -> Result<()> {
        self.inner.delete(reference).await
    }
}

#[tokio::test]
async fn a_write_race_removes_the_uncommitted_vault_item()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let server = create_server(&store).await?;
    let vault = RacingVault {
        inner: MemoryVault::default(),
        store: store.clone(),
        server: server.clone(),
    };
    let expected = store.config_snapshot(&server).await?.revision;
    let result = set_secret(
        &store,
        &vault,
        &server,
        "env.SERVICE_API_KEY",
        "test-only-race-value",
        expected,
    )
    .await;
    assert!(matches!(result, Err(ClientError::RevisionConflict)));
    assert_eq!(
        vault
            .inner
            .values
            .lock()
            .map_err(|_| "poisoned vault")?
            .len(),
        0
    );
    assert!(
        store
            .config_snapshot(&server)
            .await?
            .effective
            .get(&ConfigKey::parse("env.SERVICE_API_KEY")?)
            .is_none()
    );
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn secret_is_kept_in_the_vault_and_absent_from_sqlite_and_json()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let directory = root.path().join("state");
    let store = LocalStore::open(&directory).await?;
    let vault = MemoryVault::default();
    let server = create_server(&store).await?;
    let expected = store.config_snapshot(&server).await?.revision;
    let secret = "test-only-sensitive-value-7a93";
    set_secret(
        &store,
        &vault,
        &server,
        "env.SERVICE_API_KEY",
        secret,
        expected,
    )
    .await?;
    let snapshot = store.config_snapshot(&server).await?;
    let reference = match snapshot
        .effective
        .get(&ConfigKey::parse("env.SERVICE_API_KEY")?)
    {
        Some(ConfigValue::Secret(reference)) => reference,
        _ => return Err("expected vault reference".into()),
    };
    assert_eq!(vault.read(reference).await?.as_str(), secret);
    let report = serde_json::to_string(&store.config_report(&server, false).await?)?;
    assert!(!report.contains(secret));
    assert!(!report.contains(reference.id()));
    assert!(report.contains("\"redacted\":true"));
    store.close().await;
    let bytes = std::fs::read(directory.join("state.sqlite3"))?;
    assert!(
        !bytes
            .windows(secret.len())
            .any(|window| window == secret.as_bytes())
    );
    Ok(())
}

#[tokio::test]
async fn invalid_secret_setting_is_rejected_before_a_vault_write()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let vault = MemoryVault::default();
    let server = create_server(&store).await?;
    let expected = store.config_snapshot(&server).await?.revision;
    let error = set_secret(&store, &vault, &server, "background", "true", expected).await;
    assert!(error.is_err());
    assert_eq!(vault.values.lock().map_err(|_| "poisoned vault")?.len(), 0);
    assert_eq!(store.config_snapshot(&server).await?.revision, expected);
    store.close().await;
    Ok(())
}

async fn create_server(store: &LocalStore) -> Result<String> {
    Ok(store
        .save_connection(&SshEndpoint::parse("example.invalid", None)?, Some("dev"))
        .await?
        .id)
}
