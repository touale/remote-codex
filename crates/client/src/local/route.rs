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
    if *runtime.lease.revoked.borrow() {
        return Err(Fault::new(
            "LEASE_REVOKED",
            "session control moved to another frontend",
        ));
    }
    let prepared = runtime.native.prepare(
        method,
        params,
        runtime.permissions.full_access(),
        runtime.has_history.load(Ordering::Acquire),
        runtime.skills.mappings(),
    )?;
    execute(runtime, prepared).await
}

pub(super) async fn execute(runtime: &LocalRuntime, prepared: Prepared) -> Result<Value, Fault> {
    if *runtime.lease.revoked.borrow() || runtime.closed.borrow().is_some() {
        return Err(Fault::new(
            "SESSION_CLOSED",
            "session control is no longer available",
        ));
    }
    let operation = match prepared {
        Prepared::Immediate(value) => return Ok(value),
        Prepared::Operation(operation) => operation,
    };
    let ticket = match operation.kind {
        OperationKind::Turn => {
            runtime.bridge.check().map_err(|_| {
                Fault::unknown(
                    "execution environment is unavailable; reopen this session before continuing",
                )
            })?;
            if !runtime.has_history.load(Ordering::Acquire) {
                runtime
                    .store
                    .save_session(&runtime.binding)
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
        OperationKind::Settings(full) => runtime.permissions.begin(
            full,
            runtime
                .remote
                .identity
                .capabilities
                .iter()
                .any(|c| c == remote_codex_protocol::SESSION_PERMISSIONS_CAPABILITY),
        )?,
        OperationKind::Read => None,
    };
    let result = runtime.native.execute(operation).await;
    if let Some(ticket) = ticket {
        runtime.permissions.acknowledge(&ticket, result.is_ok())?;
        if result.is_ok() {
            runtime.permissions.confirmed(&ticket).await?;
        }
    }
    result
}
