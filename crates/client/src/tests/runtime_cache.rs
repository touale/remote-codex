use crate::{connection::SshEndpoint, runtime::RuntimeInfo, store::LocalStore};

#[tokio::test]
async fn runtime_hints_preserve_configuration_and_read_older_records()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let server = store
        .save_connection(&SshEndpoint::parse("dev.example", None)?, Some("dev"))
        .await?;
    let revision = store.config_snapshot(&server.id).await?.revision;
    // Existing installations retain their verified digest without using the old path.
    let mut tx = store.begin_write().await?;
    sqlx::query("UPDATE connections SET runtime=? WHERE id=?")
        .bind(
            serde_json::json!({
                "version":"1.2.3", "platform":"linux-x86_64", "archive_sha256":"a".repeat(64),
                "executable":"/old/runtime/bin/codex", "reused":true
            })
            .to_string(),
        )
        .bind(&server.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let previous = store
        .connection_by_id(&server.id)
        .await?
        .runtime
        .ok_or("missing hint")?;
    assert_eq!(previous.version, "1.2.3");
    assert_eq!(previous.archive_sha256, "a".repeat(64));
    let selected = RuntimeInfo {
        version: "1.2.4".into(),
        archive_sha256: "b".repeat(64),
        ..previous
    };
    store.remember_runtime(&server.id, &selected).await?;
    assert_eq!(store.config_snapshot(&server.id).await?.revision, revision);
    assert_eq!(
        store
            .connection_by_id(&server.id)
            .await?
            .runtime
            .ok_or("missing hint")?
            .reference(),
        selected.reference()
    );
    store.close().await;
    Ok(())
}
