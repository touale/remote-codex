use remote_codex_client::{
    config::{ConfigKey, ConfigValue},
    store::LocalStore,
};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{os::unix::fs::PermissionsExt, path::Path};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

async fn legacy(state: &Path, version: i64) -> TestResult<SqliteConnection> {
    std::fs::create_dir(state)?;
    std::fs::set_permissions(state, std::fs::Permissions::from_mode(0o700))?;
    let path = state.join("state.sqlite3");
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true),
    )
    .await?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    sqlx::raw_sql(include_str!("fixtures/schema_v1.sql"))
        .execute(&mut db)
        .await?;
    sqlx::raw_sql(include_str!("fixtures/legacy_state.sql"))
        .execute(&mut db)
        .await?;
    if version >= 2 {
        sqlx::raw_sql(include_str!("../src/store/migration_v2.sql"))
            .execute(&mut db)
            .await?;
        sqlx::raw_sql(include_str!("fixtures/version_two_state.sql"))
            .execute(&mut db)
            .await?;
    }
    if version >= 3 {
        sqlx::raw_sql(include_str!("../src/store/migration_v3.sql"))
            .execute(&mut db)
            .await?;
    }
    sqlx::query(&format!("PRAGMA user_version={version}"))
        .execute(&mut db)
        .await?;
    Ok(db)
}

async fn inspect(state: &Path) -> TestResult<SqliteConnection> {
    Ok(SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(state.join("state.sqlite3"))
            .read_only(true),
    )
    .await?)
}

async fn assert_current(db: &mut SqliteConnection) -> TestResult {
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sqlite_master WHERE name='scopes'")
            .fetch_one(&mut *db)
            .await?,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM server_revisions WHERE id='global'")
            .fetch_one(&mut *db)
            .await?,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&mut *db)
            .await?,
        7
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM sqlite_master WHERE name IN ('contexts','session_cache','server_histories','logins','login_operations','login_garbage')",
        )
            .fetch_one(&mut *db)
            .await?,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM settings WHERE key IN ('workspace','codex.home','codex.update_policy') OR upper(key) IN ('ENV.OPENAI_API_KEY','ENV.CODEX_API_KEY','ENV.CODEX_ACCESS_TOKEN','ENV.CODEX_AUTH_JSON')",
        )
            .fetch_one(&mut *db)
            .await?,
        0
    );
    assert!(
        sqlx::query_scalar::<_, String>("PRAGMA foreign_key_check")
            .fetch_optional(&mut *db)
            .await?
            .is_none()
    );
    Ok(())
}

async fn preserves_legacy(version: i64) -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    legacy(&state, version).await?.close().await?;
    let store = LocalStore::open(&state).await?;
    assert_eq!(store.installation_id().await?, "legacy-install");
    assert_eq!(store.find_connection("dev").await?.id, "legacy-server");
    let snapshot = store.config_snapshot("legacy-server").await?;
    assert_eq!(snapshot.revision.saved, 8);
    assert_eq!(
        snapshot.effective.get(&ConfigKey::Background),
        Some(&ConfigValue::Boolean(false))
    );
    assert_eq!(
        snapshot.effective.get(&ConfigKey::parse("env.no_proxy")?),
        Some(&ConfigValue::Text("localhost".into()))
    );
    assert!(ConfigKey::parse("env.OPENAI_API_KEY").is_err());
    assert_eq!(
        snapshot.effective.proxy_mode(),
        remote_codex_client::config::ProxyMode::Inherit
    );
    assert_eq!(
        snapshot.effective.get(&ConfigKey::parse("env.EMPTY")?),
        Some(&ConfigValue::Text(String::new()))
    );
    let proxy = ConfigKey::parse("env.https_proxy")?;
    assert_eq!(
        snapshot.effective.get(&proxy),
        Some(&ConfigValue::Text("http://global.example:7890".into()))
    );
    for server in ["other-server", "third-server"] {
        let other = store.config_snapshot(server).await?;
        assert_eq!(other.effective.get(&proxy), snapshot.effective.get(&proxy));
    }
    let mut candidates = store.workspaces("legacy-server").await?;
    candidates.sort();
    assert_eq!(
        candidates,
        vec!["/previous/terminal", "/previous/workspace"]
    );
    if version >= 2 {
        let access = store.server_access("legacy-server").await?;
        assert_eq!(
            access.identity_file.as_deref(),
            Some(Path::new("/keys/dev"))
        );
        assert!(access.remote_identity.is_none());
        assert!(access.applied_revision.is_none());
        let history = store.cached_sessions(Some("legacy-server"), false).await?;
        assert!(history.is_empty());
    }
    store.close().await;
    let mut db = inspect(&state).await?;
    assert_current(&mut db).await?;
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM credentials WHERE id='vault-reference'")
            .fetch_one(&mut db)
            .await?,
        "active"
    );
    if version >= 2 {
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT used_at FROM workspace_history WHERE path='/previous/workspace'"
            )
            .fetch_one(&mut db)
            .await?,
            123
        );
    }
    db.close().await?;
    let reopened = LocalStore::open(&state).await?;
    assert_eq!(reopened.workspaces("legacy-server").await?.len(), 2);
    reopened.close().await;
    let backups: Vec<_> = std::fs::read_dir(state.join("backups"))?.collect::<Result<_, _>>()?;
    assert_eq!(backups.len(), 1);
    assert_eq!(backups[0].metadata()?.permissions().mode() & 0o777, 0o600);
    let mut original = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(backups[0].path())
            .read_only(true),
    )
    .await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&mut original)
            .await?,
        version
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT revision FROM scopes WHERE id='global'")
            .fetch_one(&mut original)
            .await?,
        4
    );
    original.close().await?;
    Ok(())
}

#[tokio::test]
async fn version_one_preserves_identity_settings_credentials_and_directory_choices() -> TestResult {
    preserves_legacy(1).await
}

#[tokio::test]
async fn version_two_also_preserves_access_history_and_recent_timestamps() -> TestResult {
    preserves_legacy(2).await
}

#[tokio::test]
async fn fresh_database_has_only_the_current_model() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    LocalStore::open(&state).await?.close().await;
    let mut db = inspect(&state).await?;
    assert_current(&mut db).await?;
    db.close().await?;
    assert!(!state.join("backups").exists());
    Ok(())
}

#[tokio::test]
async fn version_three_freezes_global_settings_into_independent_servers() -> TestResult {
    preserves_legacy(3).await
}

#[path = "migration/backup.rs"]
mod backup;
#[path = "migration/rollback.rs"]
mod rollback;
