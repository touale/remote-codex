use remote_codex_client::{
    config::{ConfigKey, ConfigValue},
    store::LocalStore,
};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{os::unix::fs::PermissionsExt, path::Path};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

async fn version_six(state: &Path) -> TestResult<SqliteConnection> {
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
    sqlx::raw_sql(include_str!("../src/store/schema.sql"))
        .execute(&mut db)
        .await?;
    sqlx::raw_sql(include_str!("../src/store/migration_v6.sql"))
        .execute(&mut db)
        .await?;
    sqlx::raw_sql(include_str!("fixtures/version_six_state.sql"))
        .execute(&mut db)
        .await?;
    Ok(db)
}

#[tokio::test]
async fn policy_cleanup_preserves_execution_installations_sessions_and_credentials() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let mut db = version_six(&state).await?;
    let binding: String = sqlx::query_scalar("SELECT record FROM local_sessions")
        .fetch_one(&mut db)
        .await?;
    let store = LocalStore::open(&state).await?;
    assert_eq!(store.installation_id().await?, "execution-install");
    assert_eq!(
        store.find_connection("dev").await?.phase,
        "service_prepared"
    );
    assert_eq!(
        store.session_binding("thread-id").await?.session.title,
        "Existing work"
    );
    assert_eq!(store.workspaces("dev-id").await?, ["/workspace/project"]);
    let snapshot = store.config_snapshot("dev-id").await?;
    assert_eq!(snapshot.revision.saved, 12);
    assert_eq!(
        snapshot.effective.get(&ConfigKey::Background),
        Some(&ConfigValue::Boolean(false))
    );
    assert_eq!(
        snapshot.effective.get(&ConfigKey::DisconnectGraceSeconds),
        Some(&ConfigValue::Integer(42))
    );
    assert_eq!(
        snapshot.effective.get(&ConfigKey::ExecutionMode),
        Some(&ConfigValue::Text("unrestricted".into()))
    );
    assert_eq!(store.config_snapshot("test-id").await?.revision.saved, 4);
    let access = store.server_access("dev-id").await?;
    assert_eq!(access.remote_identity.as_deref(), Some("remote-install"));
    assert_eq!(
        access.service_executable.as_deref(),
        Some("/runtimes/service/fixture/server")
    );
    assert_eq!(access.service_root.as_deref(), Some("/execution-v3"));
    assert_eq!(
        access.identity_file.as_deref(),
        Some(Path::new("/keys/dev"))
    );
    assert_eq!(access.ssh_config.as_deref(), Some(Path::new("/ssh/config")));
    assert_eq!(
        access.managed_public_key.as_deref(),
        Some("ssh-ed25519 fixture")
    );
    assert_eq!(access.applied_revision, Some(11));
    let report = store.config_report("dev-id", false).await?;
    assert!(
        report
            .items
            .iter()
            .all(|item| item.setting.key != "codex.update_policy")
    );
    assert!(
        report
            .items
            .iter()
            .filter(|item| !item.setting.key.starts_with("ssh."))
            .all(|item| item.application_state == "pending_sync")
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT record FROM local_sessions")
            .fetch_one(&mut db)
            .await?,
        binding
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT digest FROM project_trust")
            .fetch_one(&mut db)
            .await?,
        "trusted-digest"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM credentials WHERE state='active'")
            .fetch_one(&mut db)
            .await?,
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key='env.SERVICE_TOKEN'")
            .fetch_one(&mut db)
            .await?,
        "service-reference"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM settings WHERE key='codex.update_policy'"
        )
        .fetch_one(&mut db)
        .await?,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&mut db)
            .await?,
        7
    );
    store.close().await;
    db.close().await?;
    let reopened = LocalStore::open(&state).await?;
    assert_eq!(reopened.config_snapshot("dev-id").await?.revision.saved, 12);
    reopened.close().await;
    let backups: Vec<_> = std::fs::read_dir(state.join("backups"))?.collect::<Result<_, _>>()?;
    assert_eq!(backups.len(), 1);
    let mut backup = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(backups[0].path())
            .read_only(true),
    )
    .await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&mut backup)
            .await?,
        6
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT value FROM settings WHERE key='codex.update_policy'"
        )
        .fetch_one(&mut backup)
        .await?,
        "manual"
    );
    backup.close().await?;
    Ok(())
}

#[tokio::test]
async fn failed_cleanup_rolls_back_policy_deletion_and_revision_changes() -> TestResult {
    for damage in [
        "UPDATE server_revisions SET revision=9223372036854775807 WHERE id='test-id'",
        "UPDATE settings SET value='invalid' WHERE key='background'",
    ] {
        let root = tempfile::tempdir()?;
        let state = root.path().join("state");
        let mut db = version_six(&state).await?;
        sqlx::query(damage).execute(&mut db).await?;
        assert!(LocalStore::open(&state).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&mut db)
                .await?,
            6
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT revision FROM server_revisions WHERE id='dev-id'")
                .fetch_one(&mut db)
                .await?,
            11
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT value FROM settings WHERE key='codex.update_policy'"
            )
            .fetch_one(&mut db)
            .await?,
            "manual"
        );
        assert_eq!(std::fs::read_dir(state.join("backups"))?.count(), 1);
        db.close().await?;
    }
    Ok(())
}
