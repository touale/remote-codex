use remote_codex_server::storage::Store;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn late_start_responses_and_duplicate_exits_do_not_resurrect_finished_jobs() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = Store::open(&root.path().join("state")).await?;
    for terminal in ["completed", "failed", "unknown"] {
        let id = store
            .record_job(
                "profile",
                "channel",
                &serde_json::json!({"processId":terminal}),
                "command",
            )
            .await?;
        store.set_job_state(&id, terminal, Some(17)).await?;
        store.set_job_state(&id, "running", None).await?;
        store.set_job_state(&id, "failed", Some(1)).await?;
        let job = store.job("profile", &id).await?;
        assert_eq!(job.state, terminal);
        assert_eq!(job.exit_code, Some(17));
    }
    assert_eq!(store.active_jobs("channel").await?, 0);
    Ok(())
}

#[tokio::test]
async fn native_process_ids_are_namespaced_and_never_replace_job_identity() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = Store::open(&root.path().join("state")).await?;
    let params = serde_json::json!({"processId":"reused","cwd":"file:///workspace","env":{}});
    let one = store
        .record_job("one", "first-channel", &params, "command")
        .await?;
    let two = store
        .record_job("two", "second-channel", &params, "command")
        .await?;
    assert_ne!(one, two);
    assert!(store.job("one", &two).await.is_err());
    let again = store
        .record_job("one", "first-channel", &params, "command")
        .await?;
    assert_ne!(one, again);
    assert_eq!(
        store.process_job("first-channel", "reused").await?,
        Some(again)
    );
    assert_eq!(store.job("one", &one).await?.process_id, "reused");
    Ok(())
}

#[tokio::test]
async fn replay_overflow_is_explicit_and_keeps_a_bounded_tail() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = Store::open(&root.path().join("state")).await?;
    store.create_channel("channel", "profile", 1).await?;
    let value = serde_json::json!({"payload":"x".repeat(1024*1024)});
    let mut cursor = 0;
    for _ in 0..10 {
        cursor = store.append_event("channel", &value).await?;
    }
    assert!(
        store
            .execution_events("channel", 0)
            .await
            .err()
            .is_some_and(|e| e.outcome_unknown)
    );
    assert_eq!(
        store.execution_events("channel", cursor - 1).await?.len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn ambiguous_operations_cannot_be_replayed_or_reused_with_different_input() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = Store::open(&root.path().join("state")).await?;
    assert!(store.begin_operation("op", "digest").await?.is_none());
    assert!(
        store
            .begin_operation("op", "digest")
            .await
            .err()
            .is_some_and(|e| e.outcome_unknown)
    );
    assert_eq!(
        store
            .begin_operation("op", "different")
            .await
            .err()
            .map(|e| e.code),
        Some("REQUEST_CONFLICT".into())
    );
    let value = serde_json::json!({"session":"original"});
    store.complete_operation("op", &value).await?;
    assert_eq!(store.begin_operation("op", "digest").await?, Some(value));
    Ok(())
}
