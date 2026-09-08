use remote_codex_client::{connection::SshEndpoint, store::LocalStore};

#[tokio::test]
async fn recent_directories_are_isolated_persistent_and_do_not_change_configuration()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let store = LocalStore::open(&state).await?;
    let one = store
        .save_connection(&SshEndpoint::parse("one", None)?, Some("one"))
        .await?;
    let two = store
        .save_connection(&SshEndpoint::parse("two", None)?, Some("two"))
        .await?;
    let before = store.config_snapshot(&one.id).await?.revision;
    store.remember_workspace(&two.id, "/other/project").await?;
    for number in 0..35 {
        store
            .remember_workspace(&one.id, &format!("/project/{number:02}"))
            .await?;
    }
    assert_eq!(store.workspaces(&one.id).await?.len(), 30);
    assert_eq!(store.workspaces(&two.id).await?, vec!["/other/project"]);
    for invalid in ["relative", "~/project", "/project\n", "/project\0"] {
        assert!(store.remember_workspace(&one.id, invalid).await.is_err());
    }
    assert_eq!(store.config_snapshot(&one.id).await?.revision, before);
    let recent = store.workspaces(&one.id).await?;
    store.close().await;
    let reopened = LocalStore::open(&state).await?;
    assert_eq!(reopened.workspaces(&one.id).await?, recent);
    reopened.remove_server(&one.id).await?;
    assert!(reopened.workspaces(&one.id).await?.is_empty());
    assert_eq!(reopened.workspaces(&two.id).await?, vec!["/other/project"]);
    reopened.close().await;
    Ok(())
}

#[tokio::test]
async fn a_display_name_matching_another_id_cannot_redirect_internal_lookups()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let one = store
        .save_connection(&SshEndpoint::parse("one", None)?, Some("one"))
        .await?;
    let two = store
        .save_connection(&SshEndpoint::parse("two", None)?, Some(&one.id))
        .await?;
    assert_eq!(
        store.connection_by_id(&one.id).await?.endpoint.host(),
        "one"
    );
    assert_eq!(store.find_connection(&one.id).await?.id, two.id);
    store.close().await;
    Ok(())
}
