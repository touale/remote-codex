use super::{Context, environment::quote, headless};
use remote_codex_client::session::{
    ApprovalDecision, ApprovalsReviewer, PermissionPreset, SessionEvent, SessionSettings,
    TurnOutcome,
};
use remote_codex_test_support::{ProbeResult, model::function};
use serde_json::json;
use std::time::Duration;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let session = headless::open(context, None, false).await?;
    let result = async {
        session.settings(SessionSettings {
            model: Some("gpt-5.4-mini".into()),
            reviewer: Some(ApprovalsReviewer::User),
            ..Default::default()
        }).await?;
        let spawn = function("spawn_agent", json!({"message":"Run the scripted child task in the inherited environment.","fork_context":false}), Some("multi_agent_v1"));
        let held = function("exec_command", json!({"cmd":"printf forbidden > agents-stopped.txt","workdir":context.environment.workspace,"sandbox_permissions":"require_escalated","justification":"Hold parent and child at an isolated approval boundary"}), None);
        context.model.script([spawn.clone(), held.clone(), held])?;
        let mut events = session.events();
        let turn = session.message("Spawn a child and run the scripted command.".into(), uuid::Uuid::new_v4().to_string(), None).await?.turn_id;
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut approvals = 0;
            while approvals < 2 {
                match events.recv().await? {
                    SessionEvent::ApprovalRequested { .. } => approvals += 1,
                    SessionEvent::TurnStarted { id, .. } if id != turn => return Err("child turn replaced parent turn".into()),
                    SessionEvent::Closed { reason } => return Err(format!("session closed: {reason:?}").into()),
                    _ => {}
                }
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }).await??;
        eprintln!("agents: parent and child approvals reached the frontend");
        if session.snapshot()?.turn.as_deref() != Some(&turn) {
            return Err("parent turn was not retained while a child requested approval".into());
        }
        session.interrupt(&turn).await?;
        for pending in session.snapshot()?.pending {
            if let SessionEvent::ApprovalRequested { request_id, .. } = pending {
                match session.approve(&request_id, ApprovalDecision::Cancel) {
                    Ok(()) => {}
                    Err(error) if error.code() == "STALE_APPROVAL" => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let SessionEvent::TurnCompleted { id, outcome, .. } = events.recv().await?
                    && id == turn {
                    if !matches!(outcome, TurnOutcome::Interrupted) { return Err("parent turn was not interrupted".into()); }
                    return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
                }
            }
        }).await??;
        eprintln!("agents: parent interruption was confirmed");
        if !context.environment.absent("agents-stopped.txt").await? || session.snapshot()?.turn.is_some() {
            return Err("stop left a command running or a turn active".into());
        }
        session.settings(SessionSettings { permissions: Some(PermissionPreset::FullAccess), ..Default::default() }).await?;
        let write = function("exec_command", json!({"cmd":"mkdir -p full-access-agents; printf verified > \"full-access-agents/$CODEX_THREAD_ID\"","workdir":context.environment.workspace}), None);
        context.model.script([spawn, write.clone(), write])?;
        if headless::turn(&session, false).await? != 0 || !session.current_settings()?.full_access {
            return Err("Full Access was not applied to the next parent turn".into());
        }
        let directory = quote(&format!("{}/full-access-agents", context.environment.workspace))?;
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let files = context.environment.command(&format!("find {directory} -maxdepth 1 -type f -printf '%f\\n'")).await?;
                if files.lines().count() == 2 && files.lines().any(|id| id == session.session().id) {
                    return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }).await??;
        eprintln!("agents: parent interruption, child approval and Full Access continuation passed without reopening the session");
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }.await;
    session.close().await;
    result
}
