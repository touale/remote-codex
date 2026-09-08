use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn binding() -> Result<SessionBinding, serde_json::Error> {
    serde_json::from_value(
        json!({"server_id":"dev","remote_identity":"host","environment_id":"remote",
        "codex_home":"/local","codex_version":"0.153.4","execution_mode":"sandboxed","revision":1,
        "session":{"id":"thread","title":"test","cwd":"/workspace","created_at":1,"updated_at":1,"archived":false,"state":"idle"}}),
    )
}
fn event() -> Value {
    json!({"id":1,"method":"item/commandExecution/requestApproval","params":{
        "threadId":"thread","turnId":"turn","itemId":"item","environmentId":"remote","kind":"command",
        "command":"/bin/sh -c 'printf 123 > 1.txt'","cwd":"/workspace with spaces",
        "availableDecisions":["accept","acceptForSession","cancel"],"proposedExecpolicyAmendment":["/bin/sh"]}})
}
fn command() -> Value {
    json!({"method":"process/start","params":{"argv":["/bin/sh","-c","printf 123 > 1.txt"],
        "cwd":"file:///workspace%20with%20spaces","env":{"CODEX_THREAD_ID":"thread"},"sandbox":null}})
}
fn accept(approvals: &Approvals) -> TestResult {
    approvals.observe(&mut event(), &binding()?, "channel")?;
    approvals.respond(&json!({"id":1,"result":{"decision":"accept"}}))?;
    Ok(())
}

#[test]
fn approval_is_exact_and_consumed_once() -> TestResult {
    let approvals = Approvals::default();
    assert!(approvals.take(&command())?.is_none());
    accept(&approvals)?;
    let grant = approvals.take(&command())?.ok_or("approval missing")?;
    assert!(grant.valid_for("channel", &grant.id, &command()));
    assert!(!grant.valid_for("other", &grant.id, &command()));
    assert!(!grant.valid_for("channel", "other", &command()));
    assert!(approvals.take(&command())?.is_none());
    Ok(())
}

#[test]
fn approval_cannot_authorize_different_command_directory_or_thread() -> TestResult {
    let approvals = Approvals::default();
    accept(&approvals)?;
    for (pointer, value) in [
        (
            "/params/argv",
            json!(["/bin/sh", "-c", "printf unexpected > 1.txt"]),
        ),
        ("/params/cwd", json!("file:///other")),
        ("/params/env/CODEX_THREAD_ID", json!("other")),
        ("/method", json!("fs/writeFile")),
    ] {
        let mut message = command();
        *message.pointer_mut(pointer).ok_or("invalid fixture")? = value;
        assert!(approvals.take(&message)?.is_none(), "{pointer}");
    }
    assert!(approvals.take(&command())?.is_some());
    Ok(())
}

#[test]
fn decline_cancel_and_unknown_responses_never_grant_execution() -> TestResult {
    let approvals = Approvals::default();
    for decision in ["decline", "cancel"] {
        approvals.observe(&mut event(), &binding()?, "channel")?;
        approvals.respond(&json!({"id":1,"result":{"decision":decision}}))?;
        assert!(approvals.take(&command())?.is_none());
    }
    approvals.respond(&json!({"id":1,"result":{"decision":"accept"}}))?;
    assert!(approvals.take(&command())?.is_none());
    Ok(())
}

#[test]
fn approvals_cannot_cross_the_bound_environment_or_thread() -> TestResult {
    for key in ["environmentId", "threadId"] {
        let mut event = event();
        event["params"][key] = json!("other");
        assert!(
            Approvals::default()
                .observe(&mut event, &binding()?, "channel")
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn only_single_command_decisions_are_exposed() -> TestResult {
    let approvals = Approvals::default();
    let mut event = event();
    approvals.observe(&mut event, &binding()?, "channel")?;
    assert_eq!(
        event["params"]["availableDecisions"],
        json!(["accept", "cancel"])
    );
    assert!(event["params"]["proposedExecpolicyAmendment"].is_null());
    assert!(
        approvals
            .respond(&json!({"id":1,"result":{"decision":"acceptForSession"}}))
            .is_err()
    );
    assert!(approvals.take(&command())?.is_none());
    Ok(())
}

#[test]
fn completion_timeout_and_shutdown_revoke_unused_grants() -> TestResult {
    let approvals = Approvals::default();
    for event in [
        json!({"method":"item/completed","params":{"item":{"id":"item"}}}),
        json!({"method":"turn/completed","params":{"turn":{"id":"turn"}}}),
    ] {
        accept(&approvals)?;
        approvals.observe(&mut event.clone(), &binding()?, "channel")?;
        assert!(approvals.take(&command())?.is_none());
    }
    accept(&approvals)?;
    approvals.0.lock().map_err(|_| "poisoned fixture")?.accepted[0].0 =
        Instant::now() - Duration::from_secs(61);
    assert!(approvals.take(&command())?.is_none());
    accept(&approvals)?;
    approvals.clear();
    assert!(approvals.take(&command())?.is_none());
    Ok(())
}
