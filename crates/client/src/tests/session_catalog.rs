use crate::{
    connection::SshEndpoint,
    session::{Session, SessionBinding},
    store::LocalStore,
};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn local_session_identity_cannot_be_rebound_to_another_environment() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let first = store
        .save_connection(&SshEndpoint::parse("dev", None)?, Some("dev"))
        .await?;
    let second = store
        .save_connection(&SshEndpoint::parse("test", None)?, Some("test"))
        .await?;
    let mut binding = SessionBinding {
        server_id: first.id.clone(),
        remote_identity: "installation-one".into(),
        environment_id: "rc_dev".into(),
        codex_home: "/local/.codex".into(),
        codex_version: "0.153.4".into(),
        execution_mode: "sandboxed".into(),
        revision: 0,
        session: Session {
            id: "local-thread".into(),
            title: "Local history".into(),
            cwd: "/workspace".into(),
            created_at: 1,
            updated_at: 2,
            archived: false,
            state: "idle".into(),
        },
    };
    store.save_session(&binding).await?;
    binding.server_id = second.id;
    assert!(store.save_session(&binding).await.is_err());
    assert_eq!(
        store.session_binding("local-thread").await?.server_id,
        first.id
    );
    store.remove_server(&first.id).await?;
    assert!(store.cached_sessions(None, false).await?.is_empty());
    store.close().await;
    Ok(())
}
