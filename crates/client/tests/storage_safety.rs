use remote_codex_client::{
    ClientError,
    config::ConfigInput,
    connection::SshEndpoint,
    store::{LocalStore, Revision},
};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn existing_client_refuses_writes_after_a_newer_schema_is_installed() -> TestResult {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let store = LocalStore::open(&state).await?;
    let record = store
        .save_connection(&SshEndpoint::parse("example.invalid", None)?, Some("dev"))
        .await?;
    let revision = store.config_snapshot(&record.id).await?.revision;
    let mut newer = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(state.join("state.sqlite3")),
    )
    .await?;
    sqlx::query("PRAGMA user_version=99")
        .execute(&mut newer)
        .await?;
    assert!(matches!(
        store
            .set_config(
                &record.id,
                "background",
                ConfigInput::Plain("false"),
                revision
            )
            .await,
        Err(ClientError::Schema)
    ));
    newer.close().await?;
    store.close().await;
    let before = fs::read(state.join("state.sqlite3"))?;
    assert!(matches!(
        LocalStore::open(&state).await,
        Err(ClientError::Schema)
    ));
    assert_eq!(fs::read(state.join("state.sqlite3"))?, before);
    Ok(())
}

#[tokio::test]
async fn state_paths_reject_symlinks_and_insecure_permissions() -> TestResult {
    let root = tempfile::tempdir()?;
    let actual = root.path().join("actual");
    fs::create_dir(&actual)?;
    fs::set_permissions(&actual, fs::Permissions::from_mode(0o700))?;
    let link = root.path().join("link");
    symlink(&actual, &link)?;
    assert!(matches!(
        LocalStore::open(&link).await,
        Err(ClientError::PrivatePath)
    ));
    fs::set_permissions(&actual, fs::Permissions::from_mode(0o755))?;
    assert!(matches!(
        LocalStore::open(&actual).await,
        Err(ClientError::PrivatePath)
    ));
    Ok(())
}

#[tokio::test]
async fn unknown_server_cannot_read_or_write_configuration() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    assert!(matches!(
        store.config_snapshot("global").await,
        Err(ClientError::NotFound)
    ));
    assert!(matches!(
        store
            .set_config(
                "global",
                "background",
                ConfigInput::Plain("false"),
                Revision { saved: 0 }
            )
            .await,
        Err(ClientError::NotFound)
    ));
    store.close().await;
    Ok(())
}
