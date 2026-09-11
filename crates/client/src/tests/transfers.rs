use crate::{
    Result,
    application::{Client, ClientOptions},
    connection::SshEndpoint,
    store::LocalStore,
    transfers::{lease::Lease, model::Status},
};
#[tokio::test]
async fn restart_restores_only_paused_metadata_and_preserves_native_grants() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let source = root.path().join("source.bin");
    std::fs::write(&source, b"\0binary")?;
    let options = ClientOptions {
        data_dir: Some(state.clone()),
        ..Default::default()
    };
    let client = Client::open(options.clone()).await?;
    let store = LocalStore::open(&state).await?;
    store
        .save_connection(
            &SshEndpoint::parse("unreachable.invalid", None)?,
            Some("dev"),
        )
        .await?;
    let task = client
        .transfers()
        .upload("dev", "/project", "", vec![source.clone()])
        .await?;
    let mut persisted = store.transfer(&task.id).await?;
    persisted.view.status = Status::Running;
    persisted.view.active = true;
    store.transfer_save(&persisted).await?;
    client.close().await;
    store.close().await;
    let client = Client::open(options).await?;
    let tasks = client.transfers().list().await?;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, Status::Paused);
    assert!(!tasks[0].active);
    let first = Lease::acquire(&state, &task.id)?;
    assert!(Lease::acquire(&state, &task.id).is_err());
    drop(first);
    assert!(Lease::acquire(&state, &task.id).is_ok());
    assert_eq!(std::fs::read(source)?, b"\0binary");
    client.close().await;
    Ok(())
}
