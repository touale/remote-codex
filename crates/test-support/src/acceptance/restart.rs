use super::{Context, headless};
use remote_codex_client::session::{
    EnvironmentState, PermissionPreset, SessionEvent, SessionSettings, TurnOutcome,
};
use remote_codex_test_support::{ProbeResult, model::function};
use serde_json::json;
use std::time::Duration;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let session = headless::open(context, None, false).await?;
    let id = session.session().id.clone();
    session
        .settings(SessionSettings {
            model: Some("gpt-6-astra".into()),
            effort: Some("low".into()),
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    context.model.script([function("exec_command", json!({"cmd":"printf once >> restart-count; sleep 20", "workdir":context.environment.workspace,"yield_time_ms":10000}), None)])?;
    let mut events = session.events();
    let mut state = session.environment();
    let original = session
        .message(
            "Run the fixed restart task.".into(),
            uuid::Uuid::new_v4().to_string(),
            None,
        )
        .await
        .map(|receipt| receipt.turn_id)?;
    tokio::time::timeout(Duration::from_secs(20), async {
        while context.environment.absent("restart-count").await? {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    context.environment.stop_service().await?;
    let mut continued = 0;
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            tokio::select! {
                event = events.recv() => match event? {
                    SessionEvent::TurnStarted { id, .. } if id != original => continued += 1,
                    SessionEvent::TurnCompleted { id, outcome: TurnOutcome::Completed, .. } if id != original => break,
                    SessionEvent::Closed { reason } => return Err(format!("restart closed session: {reason:?}").into()),
                    _ => {},
                },
                changed = state.changed() => {
                    changed?;
                    if let EnvironmentState::ActionRequired { code, message } = state.borrow().clone() {
                        return Err(format!("restart requires action: {code}: {message}").into());
                    }
                },
            }
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }).await??;
    if continued != 1
        || session.session().id != id
        || context.environment.read("restart-count").await? != "once"
    {
        return Err(
            "restart changed identity, replayed a command or duplicated continuation".into(),
        );
    }
    context.model.script([function(
        "exec_command",
        json!({"cmd":"printf recovered > restart-after", "workdir":context.environment.workspace}),
        None,
    )])?;
    headless::turn(&session, false).await?;
    if context.environment.read("restart-after").await? != "recovered" {
        return Err("restored executor did not run".into());
    }
    let requests = context.model.requests()?;
    let continuation = requests
        .iter()
        .find(|r| r.to_string().contains("[remote-codex recovery "))
        .ok_or("recovery prompt missing")?;
    if continuation["model"] != "gpt-6-astra" || continuation["reasoning"]["effort"] != "low" {
        return Err("restart lost native settings".into());
    }
    let prompt = continuation["input"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["role"] == "user")
        .flat_map(|item| item["content"].as_array().into_iter().flatten())
        .filter_map(|content| content["text"].as_str())
        .find(|text| text.starts_with("[remote-codex recovery "))
        .ok_or("automatic recovery message missing")?;
    if prompt.len() > 1024 || !prompt.contains("remote_jobs") || prompt.contains("\"jobs\"") {
        return Err(
            "recovery must query job evidence on demand, not submit it as user text".into(),
        );
    }
    pending_interaction(context, &session).await?;
    session.close().await;
    eprintln!(
        "restart: isolated supervisor restarted; same session continued once, settings and execution restored"
    );
    Ok(())
}

async fn pending_interaction(
    context: &Context,
    session: &remote_codex_client::application::SessionHandle,
) -> ProbeResult<()> {
    use remote_codex_client::session::{ApprovalDecision, ApprovalsReviewer};
    session
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::Workspace),
            reviewer: Some(ApprovalsReviewer::User),
            ..Default::default()
        })
        .await?;
    context.model.script([function("exec_command", json!({
        "cmd":"printf forbidden > stale-approval", "workdir":context.environment.workspace,
        "sandbox_permissions":"require_escalated", "justification":"Check pending approval recovery"
    }), None)])?;
    let mut events = session.events();
    session
        .message(
            "Request the isolated approval.".into(),
            uuid::Uuid::new_v4().to_string(),
            None,
        )
        .await?;
    let request = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let SessionEvent::ApprovalRequested { request_id, .. } = events.recv().await? {
                return Ok::<_, tokio::sync::broadcast::error::RecvError>(request_id);
            }
        }
    })
    .await??;
    if session.snapshot()?.pending.is_empty() {
        return Err("pending approval missing before restart".into());
    }
    context.environment.stop_service().await?;
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            if let SessionEvent::InteractionResolved { request_id } = events.recv().await?
                && request_id == request
            {
                break;
            }
        }
        Ok::<_, tokio::sync::broadcast::error::RecvError>(())
    })
    .await??;
    if !session.snapshot()?.pending.is_empty() {
        return Err("restart left stale interactions in the snapshot".into());
    }
    if session
        .approve(&request, ApprovalDecision::AcceptOnce)
        .err()
        .is_none_or(|e| e.code() != "STALE_APPROVAL")
    {
        return Err("restart retained authority for an old approval".into());
    }
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            if let SessionEvent::TurnCompleted {
                outcome: TurnOutcome::Completed,
                ..
            } = events.recv().await?
            {
                break;
            }
        }
        Ok::<_, tokio::sync::broadcast::error::RecvError>(())
    })
    .await??;
    if !context.environment.absent("stale-approval").await? {
        return Err("unapproved command ran after restart".into());
    }
    eprintln!(
        "pending interactions: restart notified resolution, cleared the snapshot and rejected stale approval"
    );
    Ok(())
}
