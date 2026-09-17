use super::{Context, headless};
use remote_codex_client::{
    application::{GoalAction, GoalStatus},
    session::{EnvironmentState, PermissionPreset, SessionEvent, SessionSettings},
};
use remote_codex_test_support::{ProbeResult, model::function};
use serde_json::json;
use std::time::Duration;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let session = headless::open(context, None, false).await?;
    session
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    context.model.script([function("exec_command",json!({"cmd":"printf once >> goal-restart-count; sleep 20","workdir":context.environment.workspace,"yield_time_ms":10000}),None)])?;
    let mut events = session.events();
    session
        .goal(GoalAction::Set {
            objective: "Run the isolated restart marker once and verify it after recovery.".into(),
            token_budget: Some(8),
        })
        .await?;
    tokio::time::timeout(Duration::from_secs(20), async {
        while context.environment.absent("goal-restart-count").await? {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    context.environment.stop_service().await?;
    let mut suspended = false;
    let mut resumed = false;
    let mut rejected_resume = false;
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            match events.recv().await? {
                SessionEvent::GoalChanged { goal: Some(goal) } => match goal.status {
                    GoalStatus::Paused => suspended = true,
                    GoalStatus::Active if suspended => resumed = true,
                    GoalStatus::BudgetLimited if resumed => break,
                    _ => {}
                },
                SessionEvent::EnvironmentChanged {
                    state: EnvironmentState::ActionRequired { code, message },
                } => return Err(format!("goal recovery requires action: {code}: {message}").into()),
                SessionEvent::EnvironmentChanged {
                    state: EnvironmentState::Reconnecting { .. },
                } if !rejected_resume && session.snapshot()?.turn.is_none() => {
                    let error = session
                        .goal(GoalAction::Resume)
                        .await
                        .err()
                        .ok_or("goal resumed before the environment was ready")?;
                    if error.code() != "ENVIRONMENT_NOT_READY" {
                        return Err(format!("unexpected goal recovery rejection: {error}").into());
                    }
                    rejected_resume = true;
                }
                SessionEvent::Closed { reason } => {
                    return Err(format!("goal recovery closed the session: {reason:?}").into());
                }
                _ => {}
            }
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    if !rejected_resume {
        return Err("goal recovery did not exercise a rejected resume".into());
    }
    if context.environment.read("goal-restart-count").await? != "once" {
        return Err("goal recovery duplicated remote execution".into());
    }
    if session
        .snapshot()?
        .goal
        .as_ref()
        .is_none_or(|g| g.status != GoalStatus::BudgetLimited)
    {
        return Err("goal budget did not stop automatic continuation".into());
    }
    session.close().await;
    eprintln!(
        "goal recovery: native goal paused across supervisor restart, resumed after recovery, stopped at budget; remote command ran once"
    );
    Ok(())
}
