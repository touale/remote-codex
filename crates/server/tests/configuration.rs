use remote_codex_protocol::{Call, RemoteConfig, Request, VERSION};
use remote_codex_server::{paths, service::Service, storage::Store};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
type TestResult = Result<(), Box<dyn std::error::Error>>;
use std::collections::BTreeMap;

#[tokio::test]
async fn invalid_configuration_never_replaces_acknowledged_values() -> TestResult {
    let root = tempfile::tempdir()?;
    let service = Service::open(&root.path().join("state")).await?;
    let config = RemoteConfig {
        revision: 1,
        values: BTreeMap::from([("background".into(), "true".into())]),
    };
    service.store.save_config("profile", &config).await?;
    let mut invalid = config.clone();
    invalid.revision += 1;
    invalid
        .values
        .insert("unknown.setting".into(), "invalid".into());
    let call = Call {
        protocol: VERSION,
        id: uuid::Uuid::new_v4().to_string(),
        profile: "profile".into(),
        expected_identity: Some(service.store.identity.clone()),
        request: Request::Configure(invalid),
    };
    assert!(
        service
            .dispatch(&call)
            .await
            .err()
            .is_some_and(|fault| fault.code == "INVALID_CONFIG")
    );
    assert!(service.store.config("profile").await? == config);
    service.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn stale_or_conflicting_config_never_replaces_acknowledged_values() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = Store::open(&root.path().join("state")).await?;
    let config = RemoteConfig {
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
async fn unsupported_formats_are_rejected_without_changing_their_bytes() -> TestResult {
    for (application, version) in [(0x52435356, 1), (0x52435356, 99), (0, 0)] {
        let root = tempfile::tempdir()?;
        let state = root.path().join("state");
        paths::private(&state)?;
        let path = state.join("execution.sqlite3");
        paths::file(&path)?;
        let mut db =
            SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path)).await?;
        sqlx::raw_sql("CREATE TABLE preserved(value TEXT); INSERT INTO preserved VALUES('keep');")
            .execute(&mut db)
            .await?;
        for (pragma, value) in [
            ("PRAGMA application_id = ", application),
            ("PRAGMA user_version = ", version),
        ] {
            sqlx::QueryBuilder::<sqlx::Sqlite>::new(pragma)
                .push(value)
                .build()
                .execute(&mut db)
                .await?;
        }
        db.close().await?;
        let before = std::fs::read(&path)?;
        assert!(
            Store::open(&state)
                .await
                .err()
                .is_some_and(|fault| fault.code == "UNSUPPORTED_SCHEMA")
        );
        assert_eq!(before, std::fs::read(&path)?);
    }
    Ok(())
}

#[tokio::test]
async fn package_selection_migration_preserves_identity_and_settings() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    paths::private(&state)?;
    let path = state.join("execution.sqlite3");
    paths::file(&path)?;
    let mut db =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&path)).await?;
    sqlx::raw_sql(include_str!("../src/storage/schema.sql"))
        .execute(&mut db)
        .await?;
    sqlx::raw_sql("INSERT INTO identity(singleton,id) VALUES(1,'preserved-identity'); PRAGMA application_id=1380143958; PRAGMA user_version=2;").execute(&mut db).await?;
    let old = serde_json::json!({"codex":"/old/managed/codex","values":{"background":"true"},"revision":7});
    sqlx::query("INSERT INTO profiles(id,config,revision) VALUES('profile',?,7)")
        .bind(old.to_string())
        .execute(&mut db)
        .await?;
    db.close().await?;
    let store = Store::open(&state).await?;
    assert_eq!(store.identity, "preserved-identity");
    let current = store.config("profile").await?;
    assert_eq!(current.revision, 7);
    assert_eq!(
        current.values.get("background").map(String::as_str),
        Some("true")
    );
    // Re-synchronizing the same revision must still be accepted after migration.
    store.save_config("profile", &current).await?;
    Ok(())
}
