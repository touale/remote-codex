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

#[tokio::test]
async fn removing_history_preserves_resumable_work_and_files() -> Result<()> {
    let root = tempfile::tempdir()?;
    let directory = root.path().join("state");
    let source = root.path().join("source.txt");
    std::fs::write(&source, "keep this file")?;
    let options = ClientOptions {
        data_dir: Some(directory.clone()),
        ..Default::default()
    };
    let client = Client::open(options.clone()).await?;
    let store = LocalStore::open(&directory).await?;
    store
        .save_connection(
            &SshEndpoint::parse("unreachable.invalid", None)?,
            Some("dev"),
        )
        .await?;
    let mut ids = Vec::new();
    for status in [
        Status::Completed,
        Status::Cancelled,
        Status::Paused,
        Status::Completed,
    ] {
        let task = client
            .transfers()
            .upload("dev", "/project", "", vec![source.clone()])
            .await?;
        let mut saved = store.transfer(&task.id).await?;
        saved.view.status = status;
        store.transfer_save(&saved).await?;
        ids.push(task.id);
    }
    let item = crate::transfers::model::Item {
        index: 0,
        parent: None,
        root: 0,
        source: "source.txt".into(),
        target: "source.txt".into(),
        stamp: remote_codex_protocol::transfer::Stamp {
            kind: remote_codex_protocol::transfer::Kind::File,
            length: 14,
            modified_ns: 0,
            device: 1,
            inode: 1,
            mode: 0o600,
        },
        token: "checkpoint".into(),
        expected: None,
        prepared: false,
        bytes: 14,
        done: true,
        skipped: false,
        choice: None,
        restart: false,
        restage: false,
    };
    store.transfer_item(&ids[0], &item).await?;
    let lease = Lease::acquire(&directory, &ids[3])?;
    let mut removed = client.transfers().remove_finished(&ids).await?;
    removed.sort();
    let mut expected = ids[..2].to_vec();
    expected.sort();
    assert_eq!(removed, expected);
    assert!(store.transfer_items(&ids[0]).await?.is_empty());
    assert!(
        client
            .transfers()
            .remove_finished(&[ids[0].clone(), ids[2].clone(), ids[3].clone()])
            .await?
            .is_empty()
    );
    drop(lease);
    assert_eq!(
        client
            .transfers()
            .remove_finished(&[ids[3].clone()])
            .await?,
        vec![ids[3].clone()]
    );
    store.close().await;
    client.close().await;
    let reopened = Client::open(options).await?;
    let remaining = reopened.transfers().list().await?;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, ids[2]);
    assert_eq!(std::fs::read_to_string(source)?, "keep this file");
    reopened.close().await;
    Ok(())
}
