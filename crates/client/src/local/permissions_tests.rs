use super::*;
use serde_json::json;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn command() -> Value {
    json!({"method":"process/start","params":{"env":{"CODEX_THREAD_ID":"thread"}}})
}
fn setup() -> Result<Permissions, Fault> {
    let permissions = Permissions::default();
    permissions.restore("channel", "thread", false)?;
    Ok(permissions)
}
fn begin(permissions: &Permissions) -> Result<String, Fault> {
    permissions.begin(Some(true), true)?.ok_or_else(invalid)
}

#[test]
fn full_access_requires_intent_native_ack_and_observed_settings_in_either_order() -> TestResult {
    for event_first in [false, true] {
        let permissions = setup()?;
        permissions.observe("thread", true)?;
        assert!(!permissions.full_access());
        let ticket = begin(&permissions)?;
        assert!(permissions.authorization(&command())?.is_none());
        if event_first {
            permissions.observe("thread", true)?;
        } else {
            permissions.acknowledge(&ticket, true)?;
        }
        assert!(!permissions.full_access());
        if event_first {
            permissions.acknowledge(&ticket, true)?;
        } else {
            permissions.observe("thread", true)?;
        }
        assert!(permissions.full_access());
        assert!(permissions.authorization(&command())?.is_some());
    }
    Ok(())
}

#[test]
fn failed_settings_and_unrelated_threads_cannot_grant_access() -> TestResult {
    let permissions = setup()?;
    let ticket = begin(&permissions)?;
    permissions.observe("another", true)?;
    permissions.acknowledge(&ticket, true)?;
    assert!(!permissions.full_access());
    permissions.acknowledge(&ticket, false)?;
    permissions.observe("thread", true)?;
    assert!(!permissions.full_access());
    assert!(permissions.begin(Some(true), false).is_err());
    Ok(())
}

#[test]
fn reverting_to_workspace_revokes_future_operations_immediately() -> TestResult {
    let permissions = setup()?;
    permissions.restore("channel", "thread", true)?;
    assert!(permissions.authorization(&command())?.is_some());
    let ticket = permissions
        .begin(Some(false), true)?
        .ok_or("missing intent")?;
    assert!(permissions.authorization(&command())?.is_none());
    permissions.observe("thread", false)?;
    permissions.acknowledge(&ticket, true)?;
    assert!(!permissions.full_access());
    Ok(())
}

#[test]
fn restored_permissions_remain_bound_to_one_channel_and_thread() -> TestResult {
    let permissions = setup()?;
    permissions.restore("channel", "thread", true)?;
    let grant = permissions
        .authorization(&command())?
        .ok_or("missing authorization")?;
    assert!(!grant.matches("another", &command()));
    let mut other = command();
    other["params"]["env"]["CODEX_THREAD_ID"] = json!("another");
    assert!(permissions.authorization(&other)?.is_none());
    assert!(grant.matches("channel", &json!({"method":"fs/writeFile"})));
    assert!(!grant.matches("channel", &json!({"method":"account/login/start"})));
    assert!(!setup()?.full_access());
    permissions.clear();
    assert!(permissions.authorization(&command())?.is_none());
    Ok(())
}
