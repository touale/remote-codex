use crate::{
    connection::SshEndpoint,
    session::{Session, SessionBinding},
    store::LocalStore,
};
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn session_bindings_and_message_times_survive_reopening_without_rebinding() -> TestResult {
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
    store
        .remember_message("local-thread", "message", 123)
        .await?;
    store
        .remember_message("local-thread", "message", 456)
        .await?;
    store.close().await;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let mut page = remote_codex_core::session::HistoryPage {
        session: store.session_binding("local-thread").await?.session,
        next_cursor: None,
        turns: vec![serde_json::from_value(
            serde_json::json!({"id":"turn", "status":"completed", "items":[
                {"id":"native", "client_id":"message", "kind":"userMessage", "text":"example"},
                {"id":"old", "kind":"userMessage", "text":"example"}
            ]}),
        )?],
    };
    store.message_times(&mut page).await?;
    assert_eq!(page.turns[0].items[0].sent_at, Some(123));
    assert_eq!(page.turns[0].items[1].sent_at, None);
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
