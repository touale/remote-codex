use crate::{connection::SshEndpoint, store::LocalStore};
use std::path::Path;

#[tokio::test]
async fn late_status_and_sync_cannot_overwrite_connection_updates()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    let first = LocalStore::open(&state).await?;
    let id = first
        .save_connection(&SshEndpoint::parse("dev.example", None)?, Some("dev"))
        .await?
        .id;
    first
        .set_ssh_options(
            &id,
            Some(Path::new("/old-key")),
            Some(Path::new("/ssh/config")),
        )
        .await?;
    first
        .set_service_installation(&id, "/old-service", "/custom-root")
        .await?;
    let second = LocalStore::open(&state).await?;
    // The first process remains alive while another changes connection and install data.
    second
        .set_managed_key(&id, Path::new("/new-key"), "new-public-key")
        .await?;
    second
        .set_service_installation(&id, "/new-service", "/default-root")
        .await?;
    second.acknowledge_config(&id, 2, "remote-identity").await?;
    first.acknowledge_config(&id, 1, "remote-identity").await?;
    first.mark_health(&id, "unavailable", 1).await?;
    let access = first.server_access(&id).await?;
    assert_eq!(access.identity_file.as_deref(), Some(Path::new("/new-key")));
    assert_eq!(access.ssh_config.as_deref(), Some(Path::new("/ssh/config")));
    assert_eq!(access.managed_public_key.as_deref(), Some("new-public-key"));
    assert_eq!(access.service_executable.as_deref(), Some("/new-service"));
    assert_eq!(access.service_root.as_deref(), Some("/custom-root"));
    assert_eq!(access.applied_revision, Some(2));
    assert_eq!(access.health, "ready");
    assert!(
        first
            .acknowledge_config(&id, 3, "other-identity")
            .await
            .is_err()
    );
    second.remove_server(&id).await?;
    assert!(first.mark_health(&id, "ready", 2).await.is_err());
    assert!(first.list_connections().await?.is_empty());
    first.close().await;
    second.close().await;
    Ok(())
}
