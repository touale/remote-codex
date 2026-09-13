use super::{Context, headless};
use remote_codex_client::session::{PermissionPreset, SessionEvent, SessionSettings, TurnOutcome};
use remote_codex_test_support::{ProbeResult, model::function};
use serde_json::{Value, json};
use std::time::Duration;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    context
        .client
        .config()
        .set("test", "background", "true", false, None)
        .await?;
    let session = headless::open(context, None, false).await?;
    session
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    context.model.script([function("exec_command", json!({"cmd":"printf once >> reconnect-count; sleep 4; printf done > reconnect-done", "workdir":context.environment.workspace,"yield_time_ms":10000}), None)])?;
    let mut events = session.events();
    let id = session
        .message(
            "Run the fixed reconnect task.".into(),
            uuid::Uuid::new_v4().to_string(),
            None,
        )
        .await
        .map(|receipt| receipt.turn_id)?;
    tokio::time::timeout(Duration::from_secs(20), async {
        while context.environment.absent("reconnect-count").await? {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    let log = std::env::var_os("REMOTE_CODEX_ACCEPTANCE_RELAYS").ok_or("relay log missing")?;
    let mut stopped = 0;
    for line in std::fs::read_to_string(log)?.lines() {
        let entry: Value = serde_json::from_str(line)?;
        if entry["kind"] == "master" {
            continue;
        }
        let pid = entry["pid"]
            .as_i64()
            .and_then(|pid| i32::try_from(pid).ok())
            .ok_or("invalid relay PID")?;
        let process = tokio::process::Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "ppid=,command="])
            .output()
            .await?;
        let process = String::from_utf8_lossy(&process.stdout);
        let expected = entry["command"].as_str().ok_or("relay command missing")?;
        if process
            .trim_start()
            .starts_with(&format!("{} ", entry["parent"]))
            && process.contains(expected)
        {
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGTERM,
            )?;
            stopped += 1;
        }
    }
    if stopped == 0 {
        return Err("no owned SSH relay was available to interrupt".into());
    }
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            match events.recv().await? {
                SessionEvent::TurnCompleted {
                    id: completed,
                    outcome: TurnOutcome::Completed,
                    ..
                } if id == completed => {
                    return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
                }
                SessionEvent::TurnCompleted { outcome, .. } => {
                    return Err(format!("reconnected turn failed: {outcome:?}").into());
                }
                SessionEvent::Closed { reason } => {
                    return Err(format!("session failed to reconnect: {reason:?}").into());
                }
                _ => {}
            }
        }
    })
    .await??;
    session.close().await;
    if context.environment.read("reconnect-count").await? != "once"
        || context.environment.read("reconnect-done").await? != "done"
    {
        return Err("reconnection lost or duplicated the command".into());
    }
    eprintln!("reconnect: owned SSH relay interrupted, command completed exactly once");
    let session = headless::open(context, None, false).await?;
    let mut states = session.environment();
    let before = context.model.requests()?.len();
    super::ssh::interrupt_master(std::process::id()).await?;
    tokio::time::timeout(Duration::from_secs(45), async {
        let mut offline = false;
        loop {
            states.changed().await?;
            match states.borrow_and_update().clone() {
                remote_codex_client::session::EnvironmentState::Reconnecting { .. } => {
                    offline = true
                }
                remote_codex_client::session::EnvironmentState::Ready if offline => break,
                remote_codex_client::session::EnvironmentState::ActionRequired {
                    message, ..
                } => return Err(message.into()),
                _ => {}
            }
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    if context.model.requests()?.len() != before {
        return Err("idle reconnection started an unsolicited task".into());
    }
    session.close().await;
    eprintln!(
        "reconnect: owned SSH master replaced; idle session restored without starting a task"
    );
    Ok(())
}
