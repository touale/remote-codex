use crate::{
    Result,
    application::{Client, ClientOptions},
    connection::SshEndpoint,
    store::LocalStore,
    workspace_lock::WorkspaceLock,
};

#[tokio::test]
async fn desktop_client_can_be_owned_by_an_async_executor() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("state");
    let client = tokio::spawn(async move {
        Client::open(ClientOptions {
            data_dir: Some(path),
            ..Default::default()
        })
        .await
    })
    .await
    .map_err(std::io::Error::other)??;
    client.close().await;
    Ok(())
}
#[tokio::test]
async fn independent_workspace_catalog_preserves_empty_and_older_workspaces() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let server = store
        .save_connection(&SshEndpoint::parse("dev", None)?, Some("dev"))
        .await?;
    for index in 0..40 {
        store
            .remember_workspace(&server.id, &format!("/workspace/{index}"))
            .await?;
    }
    assert_eq!(store.workspace_catalog().await?.len(), 40);
    store.remove_workspace(&server.id, "/workspace/2").await?;
    assert_eq!(store.workspace_catalog().await?.len(), 39);
    assert!(store.cached_sessions(None, false).await?.is_empty());
    store.close().await;
    Ok(())
}
#[test]
fn workspace_and_server_removal_wait_for_all_frontends() -> Result<()> {
    let root = tempfile::tempdir()?;
    let one = WorkspaceLock::acquire(root.path(), "server", Some("/project"), false)?;
    let two = WorkspaceLock::acquire(root.path(), "server", Some("/project"), false)?;
    assert!(WorkspaceLock::acquire(root.path(), "server", Some("/project"), true).is_err());
    assert!(WorkspaceLock::acquire(root.path(), "server", None, true).is_err());
    assert!(WorkspaceLock::acquire(root.path(), "other", Some("/project"), true).is_ok());
    one.release();
    assert!(WorkspaceLock::acquire(root.path(), "server", Some("/project"), true).is_err());
    two.release();
    assert!(WorkspaceLock::acquire(root.path(), "server", Some("/project"), true).is_ok());
    Ok(())
}
