use super::{Context, headless, ssh};
use remote_codex_client::session::{
    EnvironmentState, PermissionPreset, SessionEvent, SessionSettings, TurnOutcome,
};
use remote_codex_test_support::{ProbeResult, model::function};
use serde_json::json;
use std::{path::PathBuf, time::Duration};

pub(super) struct Offline(PathBuf);
impl Offline {
    pub(super) async fn start(parent: u32) -> ProbeResult<Self> {
        let path = PathBuf::from(
            std::env::var_os("REMOTE_CODEX_ACCEPTANCE_OUTAGE").ok_or("outage path missing")?,
        );
        std::fs::write(&path, b"offline")?;
        let offline = Self(path);
        ssh::interrupt_master(parent).await?;
        Ok(offline)
    }
}
impl Drop for Offline {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let session = headless::open(context, None, false).await?;
    let mut states = session.environment();
    let before = context.model.requests()?.len();
    let offline = Offline::start(std::process::id()).await?;
    let mut held = Some(offline);
    let release = tokio::time::sleep(Duration::from_secs(31));
    tokio::pin!(release);
    let mut attempts = 0;
    tokio::time::timeout(Duration::from_secs(65), async {
        loop {
            tokio::select! {
                _ = &mut release, if held.is_some() => { held.take(); },
                result = states.changed() => {
                    result?;
                    match states.borrow_and_update().clone() {
                        EnvironmentState::Reconnecting { attempt, .. } => attempts = attempts.max(attempt),
                        EnvironmentState::Ready if held.is_none() && attempts >= 5 => break,
                        EnvironmentState::ActionRequired { message, .. } => return Err(message.into()),
                        _ => {},
                    }
                }
            }
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }).await??;
    if context.model.requests()?.len() != before {
        return Err("offline idle session started a task".into());
    }
    session.close().await;
    eprintln!("outage: session stayed open beyond four failed connection attempts and recovered");
    cancelled(context).await?;
    messages(context).await
}

pub(super) async fn messages(context: &Context) -> ProbeResult<()> {
    for cancel in [true, false] {
        context
            .client
            .config()
            .set("test", "reconnect.max_attempts", "1", false, None)
            .await?;
        let session = headless::open(context, None, false).await?;
        headless::turn(&session, false).await?;
        let before = context.model.requests()?.len();
        let offline = Offline::start(std::process::id()).await?;
        let mut states = session.environment();
        tokio::time::timeout(Duration::from_secs(45), async {
            while !matches!(&*states.borrow(), EnvironmentState::ActionRequired { code, .. } if code == "RECOVERY_RETRIES_EXHAUSTED") {
                states.changed().await?;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }).await??;
        let snapshot = session.snapshot()?;
        if snapshot.closed || snapshot.turn.is_some() {
            return Err("exhaustion closed the session or retained a running turn".into());
        }
        context
            .client
            .config()
            .set("test", "reconnect.max_attempts", "2", false, None)
            .await?;
        {
            let message = session.message(
                "Run the new instruction after reconnecting.".into(),
                uuid::Uuid::new_v4().to_string(),
                None,
            );
            tokio::pin!(message);
            tokio::select! {
                biased;
                result = &mut message => return Err(format!("message did not wait for recovery: {result:?}").into()),
                _ = std::future::ready(()) => {},
            }
            if cancel {
                session.interrupt("").await?;
                let error = tokio::time::timeout(Duration::from_secs(2), &mut message)
                    .await?
                    .err()
                    .ok_or("cancelled message was submitted")?;
                if error.code() != "MESSAGE_CANCELLED" {
                    return Err(format!("unexpected cancellation error: {error}").into());
                }
            }
            drop(offline);
            session.retry();
            if !cancel {
                tokio::time::timeout(Duration::from_secs(45), &mut message).await??;
                context.model.wait_for(before + 1).await?;
            }
        }
        tokio::time::timeout(Duration::from_secs(45), async {
            while !matches!(*states.borrow(), EnvironmentState::Ready) {
                states.changed().await?;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await??;
        if context.model.requests()?.len() != before + usize::from(!cancel) {
            return Err(
                "message recovery duplicated a submission or resumed cancelled work".into(),
            );
        }
        session.close().await;
    }
    context
        .client
        .config()
        .set("test", "reconnect.max_attempts", "10", false, None)
        .await?;
    eprintln!(
        "message recovery: App submission waited for readiness; cancellation prevented late delivery"
    );
    Ok(())
}

async fn cancelled(context: &Context) -> ProbeResult<()> {
    let session = headless::open(context, None, false).await?;
    session
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    context.model.script([function("exec_command", json!({"cmd":"printf started > outage-cancel; sleep 20", "workdir":context.environment.workspace,"yield_time_ms":10000}), None)])?;
    let mut events = session.events();
    let id = session
        .message(
            "Run the fixed cancellation task.".into(),
            uuid::Uuid::new_v4().to_string(),
            None,
        )
        .await
        .map(|receipt| receipt.turn_id)?;
    tokio::time::timeout(Duration::from_secs(20), async {
        while context.environment.absent("outage-cancel").await? {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    let offline = Offline::start(std::process::id()).await?;
    let mut states = session.environment();
    tokio::time::timeout(Duration::from_secs(15), async {
        while matches!(*states.borrow(), EnvironmentState::Ready) {
            states.changed().await?;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    session.interrupt(&id).await?;
    drop(offline);
    session.retry();
    let mut completed = false;
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            tokio::select! {
                event = events.recv() => match event? {
                    SessionEvent::TurnStarted { id: next, .. } if next != id => return Err("cancelled task was automatically continued".into()),
                    SessionEvent::TurnCompleted { id: next, outcome: TurnOutcome::Interrupted, .. } if next == id => completed = true,
                    SessionEvent::Closed { reason } => return Err(format!("cancellation closed session: {reason:?}").into()),
                    _ => {},
                },
                _ = tokio::time::sleep(Duration::from_millis(100)) => {},
            }
            if completed && matches!(*states.borrow(), EnvironmentState::Ready) { break; }
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }).await??;
    session.close().await;
    eprintln!("outage: user cancellation survived reconnect without automatic continuation");
    Ok(())
}
