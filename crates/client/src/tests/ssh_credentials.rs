use crate::{ClientError, Result, config::SecretRef, connection::SshEndpoint, store::LocalStore};

#[tokio::test]
async fn credentials_are_isolated_atomic_and_retained_for_vault_cleanup() -> Result<()> {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let endpoint = SshEndpoint::parse("example.invalid", None)?;
    let dev = store.save_connection(&endpoint, Some("dev")).await?;
    let other = store
        .save_connection(&SshEndpoint::parse("test.invalid", None)?, Some("test"))
        .await?;
    let first = SecretRef::new("first-ssh-credential")?;
    let next = SecretRef::new("next-ssh-credential")?;
    let racing = SecretRef::new("racing-ssh-credential")?;
    store
        .reserve_ssh_credential(&dev.id, &first, "fixture-target")
        .await?;
    assert!(store.ssh_credential(&dev.id).await?.is_none());
    store.activate_ssh_credential(&dev.id, None, &first).await?;
    assert!(store.ssh_credential(&other.id).await?.is_none());
    store
        .reserve_ssh_credential(&dev.id, &next, "fixture-target")
        .await?;
    store
        .reserve_ssh_credential(&dev.id, &racing, "fixture-target")
        .await?;
    store
        .activate_ssh_credential(&dev.id, Some(&first), &next)
        .await?;
    assert!(matches!(
        store
            .activate_ssh_credential(&dev.id, Some(&first), &racing)
            .await,
        Err(ClientError::RevisionConflict)
    ));
    store.retire_ssh_credential(&racing).await?;
    assert_eq!(
        store
            .ssh_credential(&dev.id)
            .await?
            .as_ref()
            .map(SecretRef::id),
        Some(next.id())
    );
    // Configuration writes must never retire SSH credentials.
    let revision = store.config_snapshot(&dev.id).await?.revision;
    store
        .set_config(
            &dev.id,
            "background",
            crate::config::ConfigInput::Plain("false"),
            revision,
        )
        .await?;
    assert_eq!(
        store
            .ssh_credential(&dev.id)
            .await?
            .as_ref()
            .map(SecretRef::id),
        Some(next.id())
    );
    store.remove_server(&dev.id).await?;
    assert_eq!(store.retired_credentials().await?.len(), 3);
    assert!(
        store
            .activate_ssh_credential(&dev.id, None, &racing)
            .await
            .is_err()
    );
    for reference in store.retired_credentials().await? {
        store.forget_retired_credential(&reference).await?;
    }
    assert!(store.retired_credentials().await?.is_empty());
    store.close().await;
    Ok(())
}
