use super::credentials::MemoryVault;
use crate::{
    config::SecretRef,
    connection::SshEndpoint,
    credentials::{CredentialVault, cleanup, locking, set_secret},
    store::LocalStore,
};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::sync::atomic::Ordering;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn rotation_and_removal_retry_cleanup_without_touching_active_credentials() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let store = LocalStore::open(&state).await?;
    let vault = MemoryVault::default();
    let dev = store
        .save_connection(&SshEndpoint::parse("dev.example", None)?, Some("dev"))
        .await?;
    let other = store
        .save_connection(&SshEndpoint::parse("other.example", None)?, Some("other"))
        .await?;
    for server in [&dev.id, &other.id] {
        let revision = store.config_snapshot(server).await?.revision;
        set_secret(
            &store,
            &vault,
            server,
            "env.SERVICE_TOKEN",
            "first",
            revision,
        )
        .await?;
    }
    let revision = store.config_snapshot(&dev.id).await?.revision;
    set_secret(
        &store,
        &vault,
        &dev.id,
        "env.SERVICE_TOKEN",
        "replacement",
        revision,
    )
    .await?;
    vault.fail_delete.store(true, Ordering::Relaxed);
    assert!(!cleanup(&store, &vault).await?);
    assert_eq!(vault.values.lock().map_err(|_| "poisoned")?.len(), 3);
    store.remove_server(&dev.id).await?;
    assert!(!cleanup(&store, &vault).await?);
    assert!(store.find_connection("dev").await.is_err());
    vault.fail_delete.store(false, Ordering::Relaxed);
    assert!(cleanup(&store, &vault).await?);
    assert!(cleanup(&store, &vault).await?);
    assert_eq!(vault.values.lock().map_err(|_| "poisoned")?.len(), 1);
    assert!(store.find_connection("other").await.is_ok());
    store.close().await;
    Ok(())
}

#[tokio::test]
async fn maintenance_preserves_writers_recent_pending_and_unclassified_history() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let store = LocalStore::open(&state).await?;
    let vault = MemoryVault::default();
    let stale = SecretRef::new("stale-owned")?;
    let recent = SecretRef::new("recent-owned")?;
    let legacy = SecretRef::new("unclassified-legacy")?;
    let permit = locking::write(&state).await?;
    for reference in [&stale, &recent] {
        store.reserve_credential(reference).await?;
    }
    for reference in [&stale, &recent, &legacy] {
        vault.put(reference, "fixture", permit.clone()).await?;
    }
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(state.join("state.sqlite3")),
    )
    .await?;
    sqlx::query("UPDATE credentials SET created_at=0 WHERE id='stale-owned'")
        .execute(&mut db)
        .await?;
    sqlx::query("INSERT INTO credentials(id,state,created_at,managed) VALUES('unclassified-legacy','retired',0,0)")
        .execute(&mut db).await?;
    assert!(
        !cleanup(&store, &vault).await?,
        "an active writer must defer maintenance"
    );
    assert!(vault.read(&stale).await.is_ok());
    drop(permit);
    assert!(cleanup(&store, &vault).await?);
    assert!(vault.read(&stale).await.is_err());
    assert!(vault.read(&recent).await.is_ok());
    assert!(vault.read(&legacy).await.is_ok());
    db.close().await?;
    store.close().await;
    Ok(())
}
