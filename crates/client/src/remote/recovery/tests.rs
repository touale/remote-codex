use super::*;
use crate::{config::ConfigInput, connection::SshEndpoint};
use remote_codex_protocol::Fault;

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

async fn fixture() -> std::result::Result<(tempfile::TempDir, Recovery), Box<dyn std::error::Error>>
{
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let server = store
        .save_connection(&SshEndpoint::parse("example.invalid", None)?, Some("dev"))
        .await?;
    let recovery = Recovery::new(store, server.id);
    recovery.attempts().await?;
    Ok((root, recovery))
}

#[tokio::test]
async fn fixed_interval_and_budget_span_transport_and_native_recovery() -> TestResult {
    let (_root, recovery) = fixture().await?;
    tokio::time::pause();
    for attempt in 1..=10 {
        let error = match attempt {
            1 => ClientError::Ssh(255),
            2 => {
                recovery.begin_rebuild();
                // Bridge completion crosses the Fault boundary before rebuilding.
                ClientError::RemoteResponse.into_fault().into()
            }
            _ => Fault {
                code: "CODEX_RESPONSE_TIMEOUT".into(),
                message: "thread/resume timed out".into(),
                outcome_unknown: true,
            }
            .into(),
        };
        let started = tokio::time::Instant::now();
        tokio::time::timeout(Duration::from_secs(6), recovery.wait(&error))
            .await
            .map_err(|_| "automatic recovery waited for manual intervention")??;
        assert!(
            (Duration::from_secs(5)..=Duration::from_millis(5001)).contains(&started.elapsed())
        );
        assert_eq!(
            *recovery.state.borrow(),
            EnvironmentState::Reconnecting {
                attempt,
                max_attempts: 10,
                retry_in_ms: 0
            }
        );
    }
    assert!(
        tokio::time::timeout(
            Duration::from_secs(60),
            recovery.wait(&ClientError::Ssh(255))
        )
        .await
        .is_err()
    );
    assert!(
        matches!(&*recovery.state.borrow(), EnvironmentState::ActionRequired { code, message } if code == "RECOVERY_RETRIES_EXHAUSTED" && message.contains("10 attempts"))
    );
    assert_eq!(recovery.attempts().await?, (10, 10));
    tokio::time::resume();

    // CLI and App changes take effect on the next episode, including explicit Retry.
    let before = recovery.store.config_snapshot(&recovery.server).await?;
    recovery
        .store
        .set_config(
            &recovery.server,
            "reconnect.max_attempts",
            ConfigInput::Plain("2"),
            before.revision,
        )
        .await?;
    assert_eq!(recovery.attempts().await?, (10, 10));
    {
        let waiting = recovery.wait(&ClientError::Ssh(255));
        tokio::pin!(waiting);
        assert!(futures_util::poll!(&mut waiting).is_pending());
        recovery.retry();
        waiting.await?;
    }
    assert_eq!(recovery.attempts().await?, (1, 2));
    recovery.publish(EnvironmentState::Ready);
    assert!(!recovery.rebuilding());
    assert_eq!(recovery.attempts().await?, (0, 2));
    recovery.store.close().await;
    Ok(())
}

#[tokio::test]
async fn cancellation_and_unsafe_errors_do_not_start_attempts() -> TestResult {
    let (_root, recovery) = fixture().await?;
    tokio::time::pause();
    assert!(
        tokio::time::timeout(
            Duration::from_secs(2),
            recovery.wait(&ClientError::Ssh(255))
        )
        .await
        .is_err()
    );
    assert_eq!(recovery.attempts().await?, (0, 10));
    recovery.begin_rebuild();
    for fault in [
        Fault::unknown("turn/start outcome unknown"),
        Fault::new("SSH_AUTH_REQUIRED", "authentication required"),
        Fault::new("REMOTE_IDENTITY_CHANGED", "identity changed"),
        Fault::new("SKILLS_CHANGED", "skills changed"),
    ] {
        let code = fault.code.clone();
        assert!(
            tokio::time::timeout(Duration::from_secs(60), recovery.wait(&fault.into()))
                .await
                .is_err()
        );
        assert!(
            matches!(&*recovery.state.borrow(), EnvironmentState::ActionRequired { code: actual, .. } if actual == &code)
        );
        assert_eq!(recovery.attempts().await?, (0, 10));
    }
    tokio::time::resume();
    recovery.store.close().await;
    Ok(())
}

#[tokio::test]
async fn retry_clicks_only_wake_the_current_wait() -> TestResult {
    let (_root, recovery) = fixture().await?;
    tokio::time::pause();
    // A delayed click after recovery must not skip the next outage's delay.
    recovery.retry();
    let started = tokio::time::Instant::now();
    recovery.wait(&ClientError::Ssh(255)).await?;
    assert!(started.elapsed() >= Duration::from_secs(5));
    tokio::time::resume();

    // Repeated clicks wake one wait, without leaving another reset queued.
    {
        let waiting = recovery.wait(&ClientError::Ssh(255));
        tokio::pin!(waiting);
        assert!(futures_util::poll!(&mut waiting).is_pending());
        recovery.retry();
        recovery.retry();
        waiting.await?;
    }
    tokio::time::pause();
    let started = tokio::time::Instant::now();
    recovery.wait(&ClientError::Ssh(255)).await?;
    assert!(started.elapsed() >= Duration::from_secs(5));
    assert_eq!(recovery.attempts().await?, (2, 10));
    tokio::time::resume();
    recovery.store.close().await;
    Ok(())
}

#[tokio::test]
async fn exhausted_transport_returns_to_owner_before_waiting_for_a_new_message() -> TestResult {
    let (_root, recovery) = fixture().await?;
    tokio::time::pause();
    for _ in 0..10 {
        recovery.wait(&ClientError::Ssh(255)).await?;
    }
    let error = recovery
        .wait(&ClientError::Ssh(255))
        .await
        .err()
        .ok_or("transport stayed open")?;
    assert_eq!(error.code(), "RECOVERY_RETRIES_EXHAUSTED");
    assert!(!matches!(
        *recovery.state.borrow(),
        EnvironmentState::ActionRequired { .. }
    ));
    recovery.begin_rebuild();
    tokio::time::resume();
    {
        let waiting = recovery.wait(&error);
        tokio::pin!(waiting);
        assert!(futures_util::poll!(&mut waiting).is_pending());
        assert!(
            matches!(&*recovery.state.borrow(), EnvironmentState::ActionRequired { code, message } if code == "RECOVERY_RETRIES_EXHAUSTED" && message.starts_with("Connection failed after 10 attempts."))
        );
        recovery.retry();
        waiting.await?;
    }
    assert_eq!(recovery.attempts().await?, (1, 10));
    recovery.store.close().await;
    Ok(())
}
