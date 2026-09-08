use remote_codex_protocol::{Call, RemoteConfig, Request, VERSION};
use remote_codex_server::{paths, service::Service, storage::Store};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
type TestResult = Result<(), Box<dyn std::error::Error>>;
use std::collections::BTreeMap;

#[tokio::test]
async fn migrated_configuration_replaces_the_legacy_mirror_at_a_new_revision()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let service = Service::open(&root.path().join("state")).await?;
    let previous = RemoteConfig {
        codex: "/bin/sh".into(),
        revision: 11,
        values: BTreeMap::from([
            ("codex.update_policy".into(), "manual".into()),
            ("background".into(), "false".into()),
            ("disconnect_grace_seconds".into(), "42".into()),
            ("proxy.mode".into(), "direct".into()),
            ("execution.mode".into(), "unrestricted".into()),
        ]),
    };
    // Seed a profile persisted by the previous service version.
    service.store.save_config("profile", &previous).await?;
    let call = |config| Call {
        protocol: VERSION,
        id: uuid::Uuid::new_v4().to_string(),
        profile: "profile".into(),
        expected_identity: Some(service.store.identity.clone()),
        request: Request::Configure(config),
    };
    let mut cleaned = previous.clone();
    cleaned.values.remove("codex.update_policy");
    assert!(
        service
            .dispatch(&call(cleaned.clone()))
            .await
            .err()
            .is_some_and(|fault| fault.code == "REVISION_CONFLICT")
    );
    cleaned.revision += 1;
    service.dispatch(&call(cleaned.clone())).await?;
    assert!(service.store.config("profile").await? == cleaned);
    assert!(
        service
            .dispatch(&call(previous))
            .await
            .err()
            .is_some_and(|fault| fault.code == "INVALID_CONFIG")
    );
    assert!(service.store.config("profile").await? == cleaned);
    service.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn stale_or_conflicting_config_never_replaces_acknowledged_values() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = Store::open(&root.path().join("state")).await?;
    let config = RemoteConfig {
        codex: "/managed/codex".into(),
        values: BTreeMap::from([("background".into(), "true".into())]),
        revision: 2,
    };
    store.save_config("profile", &config).await?;
    store.save_config("profile", &config).await?;
    let mut stale = config.clone();
    stale.revision = 1;
    assert!(store.save_config("profile", &stale).await.is_err());
    stale.revision = 2;
    stale.values.insert("background".into(), "false".into());
    assert!(store.save_config("profile", &stale).await.is_err());
    assert!(store.config("profile").await? == config);
    stale.revision = 3;
    store.save_config("profile", &stale).await?;
    assert!(store.config("profile").await? == stale);
    Ok(())
}

#[tokio::test]
async fn foreign_database_is_rejected_without_changing_its_bytes() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    paths::private(&state)?;
    let path = state.join("execution.sqlite3");
    paths::file(&path)?;
    let mut db =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path)).await?;
    sqlx::query("CREATE TABLE unrelated(value TEXT)")
        .execute(&mut db)
        .await?;
    db.close().await?;
    let before = std::fs::read(&path)?;
    assert!(Store::open(&state).await.is_err());
    assert_eq!(before, std::fs::read(&path)?);
    Ok(())
}
