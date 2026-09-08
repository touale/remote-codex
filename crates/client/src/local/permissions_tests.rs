use super::*;
use serde_json::json;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn notification(thread: &str, full: bool) -> Value {
    json!({"method":"thread/settings/updated","params":{"threadId":thread,"threadSettings":{
        "sandboxPolicy":{"type":if full {"dangerFullAccess"} else {"workspaceWrite"}}}}})
}
fn command() -> Value {
    json!({"method":"process/start","params":{"env":{"CODEX_THREAD_ID":"thread"}}})
}
fn setup() -> Result<Permissions, Fault> {
    let permissions = Permissions::default();
    permissions.restore("channel", "thread", &json!({"type":"workspaceWrite"}))?;
    Ok(permissions)
}
fn begin(permissions: &Permissions) -> Result<String, Fault> {
    permissions
        .begin(&json!({"permissions":":danger-full-access"}), true)?
        .ok_or_else(invalid)
}

#[test]
fn full_access_requires_intent_native_ack_and_observed_settings_in_either_order() -> TestResult {
    for event_first in [false, true] {
        let permissions = setup()?;
        permissions.observe(&notification("thread", true))?;
        assert!(!permissions.full_access());
        let ticket = begin(&permissions)?;
        assert!(permissions.authorization(&command())?.is_none());
        if event_first {
            permissions.observe(&notification("thread", true))?;
        } else {
            permissions.acknowledge(&ticket, true)?;
        }
        assert!(!permissions.full_access());
        if event_first {
            permissions.acknowledge(&ticket, true)?;
        } else {
            permissions.observe(&notification("thread", true))?;
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
    permissions.observe(&notification("another", true))?;
    permissions.acknowledge(&ticket, true)?;
    assert!(!permissions.full_access());
    permissions.acknowledge(&ticket, false)?;
    permissions.observe(&notification("thread", true))?;
    assert!(!permissions.full_access());
    assert!(
        permissions
            .begin(&json!({"permissions":":danger-full-access"}), false)
            .is_err()
    );
    Ok(())
}

#[test]
fn reverting_to_workspace_revokes_future_operations_immediately() -> TestResult {
    let permissions = setup()?;
    permissions.restore("channel", "thread", &json!({"type":"dangerFullAccess"}))?;
    assert!(permissions.authorization(&command())?.is_some());
    let ticket = permissions
        .begin(&json!({"permissions":":workspace"}), true)?
        .ok_or("missing intent")?;
    assert!(permissions.authorization(&command())?.is_none());
    permissions.observe(&notification("thread", false))?;
    permissions.acknowledge(&ticket, true)?;
    assert!(!permissions.full_access());
    Ok(())
}

#[test]
fn restored_permissions_remain_bound_to_one_channel_and_thread() -> TestResult {
    let permissions = setup()?;
    permissions.restore("channel", "thread", &json!({"type":"dangerFullAccess"}))?;
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
