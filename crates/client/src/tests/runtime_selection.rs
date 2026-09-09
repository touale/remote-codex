use crate::{ClientError, connection::SshEndpoint, runtime::RuntimeInfo, store::LocalStore};

#[tokio::test]
async fn runtime_selection_advances_revision_once_and_rejects_stale_changes()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let server = store
        .save_connection(&SshEndpoint::parse("dev.example", None)?, Some("dev"))
        .await?;
    let previous = RuntimeInfo {
        version: "previous".into(),
        platform: "fixture".into(),
        executable: "/runtimes/codex/previous/bin/codex".into(),
        archive_sha256: "previous-digest".into(),
        reused: false,
    };
    store
        .activate_runtime(&server.id, server.saved_revision, &previous)
        .await?;
    let server = store.connection_by_id(&server.id).await?;
    let selected = RuntimeInfo {
        version: "validated".into(),
        executable: "/runtimes/codex/validated/bin/codex".into(),
        archive_sha256: "validated-digest".into(),
        ..previous.clone()
    };
    store
        .activate_runtime(&server.id, server.saved_revision, &selected)
        .await?;
    let updated = store.connection_by_id(&server.id).await?;
    assert_eq!(updated.saved_revision, server.saved_revision + 1);
    store
        .activate_runtime(&server.id, updated.saved_revision, &selected)
        .await?;
    assert_eq!(
        store.connection_by_id(&server.id).await?.saved_revision,
        updated.saved_revision
    );
    assert!(matches!(
        store
            .activate_runtime(&server.id, server.saved_revision, &previous)
            .await,
        Err(ClientError::RevisionConflict)
    ));
    let unchanged = store.connection_by_id(&server.id).await?;
    let (snapshot, runtime) = store.sync_snapshot(&server.id).await?;
    assert_eq!(snapshot.revision.saved, updated.saved_revision);
    assert_eq!(
        runtime.ok_or("missing runtime snapshot")?.executable,
        selected.executable
    );
    assert_eq!(unchanged.saved_revision, updated.saved_revision);
    assert_eq!(
        unchanged.runtime.ok_or("missing runtime")?.executable,
        selected.executable
    );
    store.close().await;
    Ok(())
}
