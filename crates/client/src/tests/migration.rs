use crate::store::LocalStore;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{os::unix::fs::PermissionsExt, path::Path};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

async fn fixture(state: &Path, version: i64) -> TestResult<SqliteConnection> {
    std::fs::create_dir(state)?;
    std::fs::set_permissions(state, std::fs::Permissions::from_mode(0o700))?;
    let path = state.join("state.sqlite3");
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true),
    )
    .await?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    sqlx::raw_sql(include_str!("fixtures/schema_v6.sql"))
        .execute(&mut db)
        .await?;
    sqlx::raw_sql(include_str!("fixtures/version_six_state.sql"))
        .execute(&mut db)
        .await?;
    if version == 7 {
        sqlx::raw_sql(
            "DELETE FROM settings WHERE key='codex.update_policy'; PRAGMA user_version=7;",
        )
        .execute(&mut db)
        .await?;
    }
    if version >= 8 {
        sqlx::raw_sql("DELETE FROM settings WHERE key='codex.update_policy'; DROP TABLE retained_legacy_credentials; ALTER TABLE connections DROP COLUMN phase; PRAGMA user_version=8;")
            .execute(&mut db).await?;
    }
    if version == 9 {
        sqlx::raw_sql(include_str!("../store/ssh_credentials.sql"))
            .execute(&mut db)
            .await?;
        sqlx::query("PRAGMA user_version=9")
            .execute(&mut db)
            .await?;
    }
    Ok(db)
}

async fn query(db: &mut SqliteConnection, query: &str) -> TestResult<String> {
    Ok(sqlx::query_scalar(query).fetch_one(db).await?)
}

async fn version(db: &mut SqliteConnection) -> TestResult<i64> {
    Ok(sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(db)
        .await?)
}

#[tokio::test]
async fn imports_supported_versions_without_changing_identity_access_or_native_bindings()
-> TestResult {
    for source in [6, 7, 8, 9] {
        let root = tempfile::tempdir()?;
        let state = root.path().join("state");
        let mut db = fixture(&state, source).await?;
        let binding = query(&mut db, "SELECT record FROM local_sessions").await?;
        let store = LocalStore::open(&state).await?;
        assert_eq!(version(&mut db).await?, 10);
        assert_eq!(store.installation_id().await?, "execution-install");
        let ownership: Vec<(String, i64)> =
            sqlx::query_as("SELECT id,managed FROM credentials ORDER BY id")
                .fetch_all(&mut db)
                .await?;
        assert_eq!(
            ownership,
            vec![
                ("legacy-reference".into(), 0),
                ("service-reference".into(), 1)
            ]
        );
        assert_eq!(
            store.session_binding("thread-id").await?.session.cwd,
            "/workspace/project"
        );
        assert_eq!(store.workspaces("dev-id").await?, ["/workspace/project"]);
        assert_eq!(
            query(&mut db, "SELECT record FROM local_sessions").await?,
            binding
        );
        assert_eq!(
            query(&mut db, "SELECT digest FROM project_trust").await?,
            "trusted-digest"
        );
        assert_eq!(
            query(&mut db, "SELECT identity_file FROM server_access").await?,
            "/keys/dev"
        );
        assert_eq!(
            query(&mut db, "SELECT remote_identity FROM server_access").await?,
            "remote-install"
        );
        assert_eq!(
            query(
                &mut db,
                "SELECT value FROM settings WHERE key='env.SERVICE_TOKEN'"
            )
            .await?,
            "service-reference"
        );
        assert_eq!(
            store.config_snapshot("dev-id").await?.revision.saved,
            if source == 6 { 12 } else { 11 }
        );
        assert_eq!(
            store.config_snapshot("test-id").await?.revision.saved,
            if source == 6 { 4 } else { 3 }
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM credentials WHERE state='active'")
                .fetch_one(&mut db)
                .await?,
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM sqlite_master WHERE name='retained_legacy_credentials'"
            )
            .fetch_one(&mut db)
            .await?,
            0
        );
        assert!(
            sqlx::query("PRAGMA foreign_key_check")
                .fetch_optional(&mut db)
                .await?
                .is_none()
        );
        store.close().await;
        LocalStore::open(&state).await?.close().await;
        let backups: Vec<_> =
            std::fs::read_dir(state.join("backups"))?.collect::<Result<_, _>>()?;
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].metadata()?.permissions().mode() & 0o777, 0o600);
        let mut backup = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(backups[0].path())
                .read_only(true),
        )
        .await?;
        assert_eq!(version(&mut backup).await?, source);
        backup.close().await?;
        db.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn invalid_import_rolls_back_and_backup_failure_never_mutates_source() -> TestResult {
    for damage in [
        "UPDATE server_revisions SET revision=9223372036854775807 WHERE id='test-id'",
        "UPDATE settings SET value='invalid' WHERE key='background'",
        "DELETE FROM credentials WHERE id='service-reference'",
    ] {
        let root = tempfile::tempdir()?;
        let state = root.path().join("state");
        let mut db = fixture(&state, 6).await?;
        sqlx::query(damage).execute(&mut db).await?;
        assert!(LocalStore::open(&state).await.is_err());
        assert_eq!(version(&mut db).await?, 6);
        assert_eq!(
            query(
                &mut db,
                "SELECT value FROM settings WHERE key='codex.update_policy'"
            )
            .await?,
            "manual"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT revision FROM server_revisions WHERE id='dev-id'")
                .fetch_one(&mut db)
                .await?,
            11
        );
        assert_eq!(std::fs::read_dir(state.join("backups"))?.count(), 1);
        db.close().await?;
    }
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let mut db = fixture(&state, 7).await?;
    std::os::unix::fs::symlink(root.path(), state.join("backups"))?;
    assert!(LocalStore::open(&state).await.is_err());
    assert_eq!(version(&mut db).await?, 7);
    db.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_imports_create_one_backup_including_committed_wal() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let mut db = fixture(&state, 7).await?;
    sqlx::raw_sql("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; UPDATE workspace_history SET path='/wal-project';").execute(&mut db).await?;
    let (one, two) = tokio::join!(LocalStore::open(&state), LocalStore::open(&state));
    one?.close().await;
    two?.close().await;
    let backups: Vec<_> = std::fs::read_dir(state.join("backups"))?.collect::<Result<_, _>>()?;
    assert_eq!(backups.len(), 1);
    let mut backup = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(backups[0].path())
            .read_only(true),
    )
    .await?;
    assert_eq!(
        query(&mut backup, "SELECT path FROM workspace_history").await?,
        "/wal-project"
    );
    backup.close().await?;
    db.close().await?;
    Ok(())
}
