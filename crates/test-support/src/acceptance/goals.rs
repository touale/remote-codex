use super::{Context, headless};
use remote_codex_client::{
    application::{GoalAction, GoalStatus},
    session::{EnvironmentState, PermissionPreset, SessionEvent, SessionSettings},
};
use remote_codex_test_support::{ProbeResult, model::function};
use serde_json::json;
use std::time::Duration;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    for restart in [false, true] {
        recover(context, restart).await?;
    }
    Ok(())
}

async fn recover(context: &Context, restart: bool) -> ProbeResult<()> {
    let session = headless::open(context, None, false).await?;
    session
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    let marker = if restart {
        "goal-restart-count"
    } else {
        "goal-reconnect-count"
    };
    context.model.script([function("exec_command",json!({"cmd":format!("printf once >> {marker}; sleep 20"),"workdir":context.environment.workspace,"yield_time_ms":10000}),None)])?;
    session
        .goal(GoalAction::Set {
            objective: "Run the isolated restart marker once and verify it after recovery.".into(),
            token_budget: Some(8),
        })
        .await?;
    tokio::time::timeout(Duration::from_secs(20), async {
        while context.environment.absent(marker).await? {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    let running = session
        .snapshot()?
        .turn
        .ok_or("goal turn was not running")?;
    session.goal(GoalAction::Pause).await?;
    session.goal(GoalAction::Resume).await?;
    if session.snapshot()?.turn.as_ref() != Some(&running) {
        return Err("manual goal resume replaced the running turn".into());
    }
    let mut events = session.events();
    if restart {
        context.environment.stop_service().await?;
    } else {
        super::ssh::interrupt_master(std::process::id()).await?;
    }
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
                } if !rejected_resume
                    && matches!(
                        session.snapshot()?.environment,
                        EnvironmentState::Reconnecting { .. }
                    ) =>
                {
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
    if context.environment.read(marker).await? != "once" {
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
        "goal recovery (restart={restart}): manual resume kept the active turn; interrupted goal resumed and stopped at budget; command ran once"
    );
    Ok(())
}
