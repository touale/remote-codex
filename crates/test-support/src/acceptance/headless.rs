use super::Context;
use remote_codex_client::{
    application::{OpenSession, SessionHandle},
    session::{
        ApprovalDecision, ApprovalsReviewer, PermissionPreset, SessionEvent, SessionSettings,
        TurnOutcome,
    },
};
use remote_codex_test_support::{ProbeResult, model::function};
use serde_json::json;
use std::time::Duration;

pub(super) async fn open(
    context: &Context,
    resume: Option<String>,
    takeover: bool,
) -> ProbeResult<SessionHandle> {
    Ok(context
        .client
        .sessions()
        .prepare(OpenSession {
            server: "test".into(),
            path: context.environment.workspace.clone(),
            resume,
            takeover,
            mcp_source: None,
        })
        .await?
        .open(false)
        .await?)
}

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let draft = open(context, None, false).await?;
    draft.close().await;
    if !context
        .client
        .sessions()
        .list(None, false)
        .await?
        .is_empty()
    {
        return Err("empty draft was catalogued".into());
    }
    let session = open(context, None, false).await?;
    session
        .settings(SessionSettings {
            model: Some("gpt-6-astra".into()),
            effort: Some("low".into()),
            reviewer: Some(ApprovalsReviewer::User),
            ..Default::default()
        })
        .await?;
    let command = json!({"cmd":"printf once >> approved; printf '%s' \"$https_proxy\" > proxy-value", "workdir":context.environment.workspace,
        "sandbox_permissions":"require_escalated","justification":"Write the isolated acceptance marker"});
    context
        .model
        .script([function("exec_command", command, None)])?;
    let approved = turn(&session, true).await?;
    if approved != 1 {
        return Err(format!("expected one approval, observed {approved}").into());
    }
    if context.environment.read("approved").await? != "once" {
        return Err("approved command did not run exactly once".into());
    }
    if let Some(proxy) = &context.environment.args.proxy
        && context.environment.read("proxy-value").await? != *proxy
    {
        return Err("server proxy was not inherited by remote execution".into());
    }
    for request in context.model.requests()? {
        if request["model"] != "gpt-6-astra"
            || request.pointer("/reasoning/effort") != Some(&json!("low"))
        {
            return Err("model or effort selection was not used by the native request".into());
        }
    }
    session
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    context.model.script([function("exec_command", json!({"cmd":"apply_patch <<'PATCH'\n*** Begin Patch\n*** Add File: full-access.txt\n+verified\n*** End Patch\nPATCH","workdir":context.environment.workspace}), None)])?;
    if turn(&session, false).await? != 0
        || context.environment.read("full-access.txt").await? != "verified\n"
    {
        return Err("Full Access patch failed".into());
    }
    let id = session.session().id.clone();
    let before = context.model.requests()?.len();
    session.close().await;
    let resumed = open(context, Some(id.clone()), false).await?;
    if context.model.requests()?.len() != before {
        return Err("resume replayed a model request".into());
    }
    resumed
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::Workspace),
            reviewer: Some(ApprovalsReviewer::User),
            ..Default::default()
        })
        .await?;
    context.model.script([function("exec_command", json!({"cmd":"printf forbidden > cancelled", "workdir":context.environment.workspace,"sandbox_permissions":"require_escalated","justification":"Verify cancellation"}), None)])?;
    if turn(&resumed, false).await? != 1 || !context.environment.absent("cancelled").await? {
        return Err("cancellation did not preserve the workspace".into());
    }
    let takeover = open(context, Some(id.clone()), true).await?;
    if resumed
        .message(
            "must not be sent".into(),
            uuid::Uuid::new_v4().to_string(),
            None,
        )
        .await
        .map(|receipt| receipt.turn_id)
        .is_ok()
    {
        return Err("revoked frontend retained control".into());
    }
    if resumed.interrupt("obsolete").await.is_ok() {
        return Err("revoked frontend accepted an interruption".into());
    }
    takeover.close().await;
    resumed.close().await;
    let history = context.client.sessions().read(&id, None).await?;
    if history.turns.len() != 3 {
        return Err(format!("expected 3 persisted turns, got {}", history.turns.len()).into());
    }
    if std::path::Path::new(&context.environment.workspace).exists() {
        return Err("remote workspace was created locally".into());
    }
    if context.environment.home.join("environments.toml").exists() {
        return Err("native environment registry was modified".into());
    }
    eprintln!("headless: approvals, settings, patch, resume, takeover and local isolation passed");
    Ok(())
}

pub(super) async fn turn(session: &SessionHandle, accept: bool) -> ProbeResult<usize> {
    let mut events = session.events();
    let id = session
        .message(
            "Run the scripted acceptance task.".into(),
            uuid::Uuid::new_v4().to_string(),
            None,
        )
        .await
        .map(|receipt| receipt.turn_id)?;
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut approvals = 0;
        loop {
            match events.recv().await? {
                SessionEvent::ApprovalRequested { request_id, .. } => {
                    approvals += 1;
                    if !session.snapshot()?.pending.iter().any(|event| matches!(event,
                        SessionEvent::ApprovalRequested { request_id: pending, .. } if pending == &request_id)) {
                        return Err("approval was not included in the interaction snapshot".into());
                    }
                    session.approve(
                        &request_id,
                        if accept {
                            ApprovalDecision::AcceptOnce
                        } else {
                            ApprovalDecision::Cancel
                        },
                    )?;
                    if !session.snapshot()?.pending.is_empty() {
                        return Err("completed approval remains in the interaction snapshot".into());
                    }
                    if session.approve(&request_id, ApprovalDecision::Cancel).err().is_none_or(|e| e.code() != "STALE_APPROVAL") {
                        return Err("an already resolved approval was accepted again".into());
                    }
                }
                SessionEvent::TurnCompleted {
                    id: completed,
                    outcome,
                    ..
                } if completed == id => {
                    return match outcome {
                        TurnOutcome::Completed => Ok(approvals),
                        TurnOutcome::Interrupted if !accept && approvals > 0 => Ok(approvals),
                        other => Err(format!("unexpected turn outcome: {other:?}").into()),
                    };
                }
                SessionEvent::InteractionRequired { kind, .. } => {
                    return Err(format!("unexpected native interaction: {kind}").into());
                }
                SessionEvent::Closed { reason } => {
                    return Err(format!("session closed: {reason:?}").into());
                }
                _ => {}
            }
        }
    })
    .await?
}
