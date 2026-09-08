use super::*;
use remote_codex_client::{config::ConfigInput, connection::SshEndpoint};

#[tokio::test]
async fn backup_includes_committed_wal_and_upgrade_holds_server_isolation() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let mut original = legacy(&state, 3).await?;
    sqlx::raw_sql("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .execute(&mut original)
        .await?;
    sqlx::query("UPDATE settings SET value='wal-value' WHERE key='env.EMPTY'")
        .execute(&mut original)
        .await?;
    assert!(state.join("state.sqlite3-wal").metadata()?.len() > 0);
    let store = LocalStore::open(&state).await?;
    let key = ConfigKey::parse("env.EMPTY")?;
    assert_eq!(
        store
            .config_snapshot("legacy-server")
            .await?
            .effective
            .get(&key),
        Some(&ConfigValue::Text("wal-value".into()))
    );
    let path = std::fs::read_dir(state.join("backups"))?
        .next()
        .ok_or("missing backup")??
        .path();
    let mut backup =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(path).read_only(true))
            .await?;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE scope='global' AND key='env.EMPTY'"
        )
        .fetch_one(&mut backup)
        .await?,
        "wal-value"
    );
    backup.close().await?;
    let before = store.config_snapshot("third-server").await?.revision;
    let own = store.config_snapshot("other-server").await?.revision;
    store
        .set_config(
            "other-server",
            "env.EMPTY",
            ConfigInput::Plain("changed"),
            own,
        )
        .await?;
    assert_eq!(
        store.config_snapshot("third-server").await?.revision,
        before
    );
    assert_eq!(
        store
            .config_snapshot("third-server")
            .await?
            .effective
            .get(&key),
        Some(&ConfigValue::Text("wal-value".into()))
    );
    store.remove_server("other-server").await?;
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT state FROM credentials WHERE id='shared-reference'"
        )
        .fetch_one(&mut original)
        .await?,
        "active"
    );
    assert!(ConfigKey::parse("env.OPENAI_API_KEY").is_err());
    let new = store
        .save_connection(&SshEndpoint::parse("new", None)?, Some("new"))
        .await?;
    let fresh = store.config_snapshot(&new.id).await?;
    assert_eq!(
        fresh.effective.proxy_mode(),
        remote_codex_client::config::ProxyMode::Direct
    );
    assert!(fresh.effective.get(&key).is_none());
    store.close().await;
    original.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_openers_commit_one_migration_and_one_backup() -> TestResult {
    for _ in 0..8 {
        concurrent_openers().await?;
    }
    Ok(())
}

async fn concurrent_openers() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    legacy(&state, 3).await?.close().await?;
    let (one, two) = tokio::join!(LocalStore::open(&state), LocalStore::open(&state));
    let (one, two) = (one?, two?);
    assert_eq!(
        one.config_snapshot("legacy-server").await?.revision,
        two.config_snapshot("legacy-server").await?.revision
    );
    one.close().await;
    two.close().await;
    assert_eq!(std::fs::read_dir(state.join("backups"))?.count(), 1);
    Ok(())
}
