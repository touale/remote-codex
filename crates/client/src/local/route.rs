use super::LocalRuntime;
use remote_codex_adapter::thread::{OperationKind, Prepared};
use remote_codex_protocol::Fault;
use serde_json::Value;
use std::sync::atomic::Ordering;

pub(super) async fn request(
    runtime: &LocalRuntime,
    method: &str,
    params: Value,
) -> Result<Value, Fault> {
    if runtime.closed.borrow().is_some() {
        return Err(Fault::new(
            "SESSION_CLOSED",
            "session control is no longer available",
        ));
    }
    if *runtime.lease.revoked.borrow() {
        return Err(Fault::new(
            "LEASE_REVOKED",
            "session control moved to another frontend",
        ));
    }
    let generation = runtime.current().map_err(fault)?;
    let interrupted = if method == "turn/interrupt" {
        params["turnId"].as_str().map(str::to_owned)
    } else {
        None
    };
    let plan_change =
        method == "thread/settings/update" && params["collaborationMode"]["mode"] == "plan";
    let prepared = generation.native.prepare(
        method,
        params,
        runtime.has_history.load(Ordering::Acquire),
        generation.skills.mappings(),
    )?;
    if matches!(method, "thread/goal/set" | "thread/goal/clear") {
        runtime
            .intent
            .lock()
            .map_err(|_| Fault::new("SESSION_STATE", "Session state unavailable"))?
            .resume_goal = None;
    }
    if plan_change {
        runtime.pause_goal().await.map_err(fault)?;
    }
    if let Some(turn) = interrupted {
        runtime.pause_goal().await.map_err(fault)?;
        runtime.cancel_continuation(&turn).map_err(fault)?;
        if runtime.recovery.ready().is_err() {
            let _ = generation.native.queue_interrupt(&turn);
            return Ok(serde_json::json!({}));
        }
    }
    execute(runtime, &generation, prepared).await
}

pub(super) async fn execute(
    runtime: &LocalRuntime,
    generation: &super::Generation,
    prepared: Prepared,
) -> Result<Value, Fault> {
    dispatch(runtime, generation, prepared, None).await
}

pub(super) async fn execute_recovery(
    runtime: &LocalRuntime,
    generation: &super::Generation,
    prepared: Prepared,
    interrupted_turn: &str,
) -> Result<Value, Fault> {
    dispatch(runtime, generation, prepared, Some(interrupted_turn)).await
}

async fn dispatch(
    runtime: &LocalRuntime,
    generation: &super::Generation,
    prepared: Prepared,
    recovery_turn: Option<&str>,
) -> Result<Value, Fault> {
    if *runtime.lease.revoked.borrow() || runtime.closed.borrow().is_some() {
        return Err(Fault::new(
            "SESSION_CLOSED",
            "session control is no longer available",
        ));
    }
    if runtime.current().map_err(fault)?.bridge.channel != generation.bridge.channel {
        return Err(Fault::new(
            "STALE_GENERATION",
            "request belongs to a previous execution environment",
        ));
    }
    let operation = match prepared {
        Prepared::Immediate(value) => return Ok(value),
        Prepared::Operation(operation) => operation,
    };
    let _turn = if matches!(
        operation.kind,
        OperationKind::Turn
            | OperationKind::Steer
            | OperationKind::GoalStart
            | OperationKind::GoalDefine
    ) {
        Some(runtime.turn_gate.lock().await)
    } else {
        None
    };
    if *runtime.lease.revoked.borrow()
        || runtime.closed.borrow().is_some()
        || runtime.current().map_err(fault)?.bridge.channel != generation.bridge.channel
    {
        return Err(Fault::new(
            "SESSION_CLOSED",
            "session control changed before dispatch",
        ));
    }
    if let Some(turn) = recovery_turn
        && runtime
            .intent
            .lock()
            .map_err(|_| Fault::new("SESSION_STATE", "session state unavailable"))?
            .is_cancelled(turn)
    {
        return Err(Fault::new(
            "RECOVERY_CANCELLED",
            "automatic continuation was cancelled",
        ));
    }
    let ticket = match operation.kind {
        OperationKind::Turn
        | OperationKind::Steer
        | OperationKind::GoalStart
        | OperationKind::GoalDefine => {
            if operation.kind != OperationKind::GoalDefine
                && !(recovery_turn.is_some()
                    && matches!(
                        *runtime.recovery.state.borrow(),
                        remote_codex_core::session::EnvironmentState::Recovering { .. }
                    ))
            {
                runtime.recovery.ready().map_err(fault)?;
            }
            if operation.kind != OperationKind::GoalDefine {
                generation.bridge.check().map_err(fault)?;
            }
            generation
                .native
                .validate_execution(&operation, generation.permissions.full_access())?;
            if !runtime.has_history.load(Ordering::Acquire) {
                runtime
                    .store
                    .save_session(generation.native.binding())
                    .await
                    .map_err(|_| {
                        Fault::new(
                            "SESSION_STORAGE",
                            "could not persist session binding; no turn was submitted",
                        )
                    })?;
                runtime.has_history.store(true, Ordering::Release);
            }
            None
        }
        OperationKind::Settings(full) => {
            runtime.recovery.ready().map_err(fault)?;
            generation.permissions.begin(
                full,
                runtime
                    .remote
                    .identity
                    .capabilities
                    .iter()
                    .any(|c| c == remote_codex_protocol::SESSION_PERMISSIONS_CAPABILITY),
            )?
        }
        OperationKind::Read | OperationKind::GoalControl => None,
    };
    if let Some(id) = operation.client_message_id() {
        runtime
            .store
            .remember_message(&runtime.binding.session.id, id, super::status::now())
            .await
            .map_err(fault)?;
    }
    let result = generation.native.execute(operation).await;
    if let Some(ticket) = ticket {
        generation
            .permissions
            .acknowledge(&ticket, result.is_ok())?;
        if result.is_ok() {
            generation.permissions.confirmed(&ticket).await?;
        }
    }
    result
}

fn fault(error: crate::ClientError) -> Fault {
    if let crate::ClientError::RemoteFault(code, message, outcome_unknown) = error {
        return Fault {
            code,
            message,
            outcome_unknown,
        };
    }
    Fault {
        code: error.code().into(),
        message: error.to_string(),
        outcome_unknown: error.outcome_is_unknown(),
    }
}
