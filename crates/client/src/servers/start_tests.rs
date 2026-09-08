use super::*;
use remote_codex_protocol::Fault;
use std::{cell::RefCell, os::unix::process::ExitStatusExt};
type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

#[tokio::test]
async fn waiting_start_retries_and_continues_with_the_same_transport_callback() -> TestResult {
    let events = RefCell::new(Vec::new());
    let mut calls = 0;
    retry(
        "dev",
        Duration::from_secs(3),
        &|event| events.borrow_mut().push(event),
        async || {
            calls += 1;
            Ok(if calls == 1 {
                ServiceStart::Waiting(ServiceActivity {
                    channels: 1,
                    jobs: 0,
                })
            } else {
                ServiceStart::Ready
            })
        },
    )
    .await?;
    assert_eq!(calls, 2);
    assert!(matches!(
        events.borrow().as_slice(),
        [
            PrepareEvent::ServiceWaiting(_),
            PrepareEvent::Stage(PrepareStage::StartService)
        ]
    ));
    Ok(())
}

#[tokio::test]
async fn prolonged_activity_is_a_known_busy_result_with_retry_guidance() -> TestResult {
    let result = retry("dev server", Duration::ZERO, &|_| {}, async || {
        Ok(ServiceStart::Waiting(ServiceActivity {
            channels: 2,
            jobs: 3,
        }))
    })
    .await;
    let error = result.err().ok_or("expected busy result")?;
    assert_eq!(error.code(), "SERVICE_UPDATE_BUSY");
    assert_eq!(error.exit_code(), 6);
    assert!(!error.outcome_is_unknown());
    assert!(
        error
            .to_string()
            .contains("Wait for running commands to finish")
    );
    assert!(error.to_string().contains("retry this command"));
    Ok(())
}

#[test]
fn structured_service_failure_preserves_its_code_instead_of_ssh_exit_seven() -> TestResult {
    let bytes = serde_json::to_vec(&ServiceStart::Failed(Fault::new(
        "INSECURE_PATH",
        "service directory is not private",
    )))?;
    let value = decode(std::process::ExitStatus::from_raw(7 << 8), &bytes, b"")?;
    let ServiceStart::Failed(fault) = value else {
        return Err("lost failure details".into());
    };
    let error: ClientError = fault.into();
    assert_eq!(error.code(), "INSECURE_PATH");
    assert_eq!(
        error.to_string(),
        "INSECURE_PATH: service directory is not private"
    );
    Ok(())
}

#[test]
fn ssh_transport_failures_cannot_masquerade_as_service_results() {
    assert!(matches!(
        decode(
            std::process::ExitStatus::from_raw(255 << 8),
            b"",
            b"connection lost"
        ),
        Err(ClientError::Ssh(255))
    ));
    assert!(
        decode(
            std::process::ExitStatus::from_raw(7 << 8),
            br#"{"status":"ready"}"#,
            b""
        )
        .is_err()
    );
}
