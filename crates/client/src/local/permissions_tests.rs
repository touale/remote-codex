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

#[tokio::test]
async fn full_access_requires_intent_native_ack_and_observed_settings_in_either_order() -> TestResult
{
    for event_first in [false, true] {
        let permissions = setup()?;
        permissions.observe("thread", true)?;
        assert!(!permissions.full_access());
        let ticket = begin(&permissions)?;
        assert!(permissions.authorization(&command()).await?.is_none());
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
        assert!(permissions.authorization(&command()).await?.is_some());
    }
    Ok(())
}

#[tokio::test]
async fn failed_settings_and_unrelated_threads_cannot_grant_access() -> TestResult {
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

#[tokio::test]
async fn reverting_to_workspace_revokes_future_operations_immediately() -> TestResult {
    let permissions = setup()?;
    permissions.restore("channel", "thread", true)?;
    assert!(permissions.authorization(&command()).await?.is_some());
    let ticket = permissions
        .begin(Some(false), true)?
        .ok_or("missing intent")?;
    assert!(permissions.authorization(&command()).await?.is_none());
    permissions.observe("thread", false)?;
    permissions.acknowledge(&ticket, true)?;
    assert!(!permissions.full_access());
    Ok(())
}

#[tokio::test]
async fn restored_permissions_remain_bound_to_one_channel_and_thread() -> TestResult {
    let permissions = setup()?;
    permissions.restore("channel", "thread", true)?;
    let grant = permissions
        .authorization(&command())
        .await?
        .ok_or("missing authorization")?;
    assert!(!grant.matches("another", &command()));
    let mut other = command();
    other["params"]["env"]["CODEX_THREAD_ID"] = json!("another");
    assert!(permissions.authorization(&other).await?.is_none());
    assert!(grant.matches("channel", &json!({"method":"fs/writeFile"})));
    assert!(!grant.matches("channel", &json!({"method":"account/login/start"})));
    assert!(!setup()?.full_access());
    permissions.clear();
    assert!(permissions.authorization(&command()).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn native_ancestry_scopes_children_and_revocation_to_the_current_generation() -> TestResult {
    use remote_codex_adapter::program::Launch;
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir()?;
    let path = home.path().join("codex-fixture");
    std::fs::write(
        &path,
        r##"#!/bin/sh
while IFS= read -r request; do
  id=$(printf '%s' "$request" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  [ -n "$id" ] || continue
  case "$request" in
    *'"method":"initialize"'*) printf '{"id":%s,"result":{}}\n' "$id" ;;
    *'"method":"thread/read"'*)
      printf '%s\n' "$request" >> reads
      thread=$(printf '%s' "$request" | sed -n 's/.*"threadId":"\([^"]*\)".*/\1/p')
      case "$thread" in
        child) parent=thread ;;
        grandchild) parent=child ;;
        cycle) parent=cycle ;;
        impostor) thread=wrong; parent=thread ;;
        *) parent=unrelated ;;
      esac
      printf '{"id":%s,"result":{"thread":{"id":"%s","source":{"subAgent":{"thread_spawn":{"parent_thread_id":"%s"}}}}}}\n' "$id" "$thread" "$parent" ;;
  esac
done
"##,
    )?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    let native = Codex::start(&Launch::new(&path), home.path()).await?;
    let result: TestResult = async {
        let permissions = setup()?;
        permissions.restore("channel", "thread", true)?;
        permissions.attach(native.clone())?;
        let mut child = command();
        child["params"]["env"]["CODEX_THREAD_ID"] = json!("grandchild");
        let grant = permissions
            .authorization(&child)
            .await?
            .ok_or("child grant missing")?;
        assert_eq!(grant.thread, "grandchild");
        assert!(grant.matches("channel", &child));
        assert!(!grant.matches("another", &child));
        let reads = std::fs::read_to_string(home.path().join("reads"))?;
        assert_eq!(reads.lines().count(), 2);
        assert!(
            reads
                .lines()
                .all(|line| line.contains("\"includeTurns\":false"))
        );
        assert!(permissions.authorization(&child).await?.is_some());
        assert_eq!(std::fs::read_to_string(home.path().join("reads"))?, reads);
        assert!(!permissions.contains("cycle").await?);
        assert!(!permissions.contains("foreign").await?);
        assert!(permissions.contains("impostor").await.is_err());
        permissions.observe("child", false)?;
        assert!(permissions.authorization(&child).await?.is_some());
        permissions.begin(Some(false), true)?;
        assert!(permissions.authorization(&child).await?.is_none());
        permissions.restore("new-channel", "thread", true)?;
        assert!(!permissions.contains("grandchild").await?);
        assert!(permissions.authorization(&child).await?.is_none());
        permissions.clear();
        assert!(!permissions.contains("thread").await?);
        Ok(())
    }
    .await;
    native.shutdown().await;
    result
}
