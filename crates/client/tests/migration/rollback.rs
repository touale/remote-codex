use super::*;
use std::os::unix::fs::symlink;

#[tokio::test]
async fn failed_upgrade_rolls_back_all_migration_steps() -> TestResult {
    for version in [1, 2] {
        let root = tempfile::tempdir()?;
        let state = root.path().join("state");
        let mut db = legacy(&state, version).await?;
        // Force the v3 directory copy to fail after the first migration writes.
        sqlx::query("ALTER TABLE contexts RENAME COLUMN workspace TO damaged")
            .execute(&mut db)
            .await?;
        db.close().await?;
        assert!(LocalStore::open(&state).await.is_err());
        let mut db = inspect(&state).await?;
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&mut db)
                .await?,
            version
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key='workspace'")
                .fetch_one(&mut db)
                .await?,
            "/previous/workspace"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM contexts")
                .fetch_one(&mut db)
                .await?,
            3
        );
        if version == 1 {
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM sqlite_master WHERE name='server_access'"
                )
                .fetch_one(&mut db)
                .await?,
                0
            );
        }
        db.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn invalid_configuration_rolls_back_version_three_upgrade() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let mut db = legacy(&state, 3).await?;
    sqlx::query("UPDATE settings SET value='not-a-proxy' WHERE key='env.https_proxy'")
        .execute(&mut db)
        .await?;
    db.close().await?;
    assert!(LocalStore::open(&state).await.is_err());
    let mut db = inspect(&state).await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&mut db)
            .await?,
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM scopes WHERE id='global'")
            .fetch_one(&mut db)
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM settings WHERE scope='other-server'")
            .fetch_one(&mut db)
            .await?,
        0
    );
    db.close().await?;
    Ok(())
}

#[tokio::test]
async fn backup_failure_prevents_any_schema_changes() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    legacy(&state, 3).await?.close().await?;
    let external = root.path().join("external");
    std::fs::create_dir(&external)?;
    symlink(&external, state.join("backups"))?;
    assert!(LocalStore::open(&state).await.is_err());
    assert_eq!(std::fs::read_dir(&external)?.count(), 0);
    let mut db = inspect(&state).await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&mut db)
            .await?,
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM scopes WHERE id='global'")
            .fetch_one(&mut db)
            .await?,
        1
    );
    db.close().await?;
    Ok(())
}

#[tokio::test]
async fn missing_shared_credential_reference_prevents_migration_commit() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let mut db = legacy(&state, 3).await?;
    sqlx::query("DELETE FROM credentials WHERE id='shared-reference'")
        .execute(&mut db)
        .await?;
    db.close().await?;
    assert!(matches!(
        LocalStore::open(&state).await,
        Err(remote_codex_client::ClientError::Credentials)
    ));
    let mut db = inspect(&state).await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&mut db)
            .await?,
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM settings WHERE scope='global'")
            .fetch_one(&mut db)
            .await?,
        5
    );
    db.close().await?;
    assert_eq!(std::fs::read_dir(state.join("backups"))?.count(), 1);
    Ok(())
}
