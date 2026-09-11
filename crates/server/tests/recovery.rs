use remote_codex_server::storage::Store;
use serde_json::json;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn restart_preserves_identity_evidence_and_unknown_outcomes() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = root.path().join("service");
    let store = Store::open(&path).await?;
    let identity = store.identity.clone();
    let instance = store.instance.clone();
    store.create_channel("channel", "owner", 1).await?;
    store.begin_operation("channel/pending", "digest").await?;
    store
        .begin_operation("channel/completed", "digest2")
        .await?;
    store
        .complete_operation("channel/completed", &json!({"ok":true}))
        .await?;
    let cursor = store
        .append_event("channel", &json!({"result":"retained"}))
        .await?;
    let job = store
        .record_job(
            "owner",
            "channel",
            &json!({"processId":"process","cwd":"file:///workspace"}),
            "command",
        )
        .await?;
    drop(store);
    let reopened = Store::open(&path).await?;
    assert_eq!(reopened.identity, identity);
    assert_ne!(reopened.instance, instance);
    let status = reopened.inspect_execution("owner", "channel").await?;
    assert_eq!(status.state, "lost");
    assert_eq!(
        reopened
            .inspect_execution("other", "channel")
            .await
            .err()
            .ok_or("expected denial")?
            .code,
        "EXECUTION_NOT_FOUND"
    );
    assert_eq!(status.reason.as_deref(), Some("service_restarted"));
    assert_eq!(status.unknown_operations, 1);
    assert_eq!(reopened.job("owner", &job).await?.state, "unknown");
    assert_eq!(reopened.execution_events("channel", 0).await?[0].0, cursor);
    assert_eq!(
        reopened
            .begin_operation("channel/completed", "digest2")
            .await?,
        Some(json!({"ok":true}))
    );
    assert!(
        reopened
            .begin_operation("channel/pending", "digest")
            .await
            .err()
            .ok_or("expected unknown outcome")?
            .outcome_unknown
    );
    Ok(())
}

#[tokio::test]
async fn evidence_retention_is_bounded_and_does_not_match_another_channel_by_case() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = Store::open(&root.path().join("service")).await?;
    store.create_channel("abc", "one", 1).await?;
    store.begin_operation("abc/pending", "one").await?;
    store.mark_channel_lost("abc").await?;
    store.create_channel("ABC", "two", 1).await?;
    store.begin_operation("ABC/pending", "two").await?;
    for number in 0..34 {
        let channel = format!("closed-{number}");
        store.create_channel(&channel, "one", 1).await?;
        store
            .append_event(&channel, &json!({"result":"bounded"}))
            .await?;
        store.mark_channel_lost(&channel).await?;
    }
    store.create_channel("new", "one", 1).await?;
    let expired = store.inspect_execution("one", "abc").await?;
    assert!(!expired.evidence_available);
    assert_eq!(expired.reason.as_deref(), Some("evidence_expired"));
    let active = store.inspect_execution("two", "ABC").await?;
    assert_eq!(active.state, "running");
    assert_eq!(active.unknown_operations, 1);
    assert!(active.evidence_available);
    Ok(())
}
