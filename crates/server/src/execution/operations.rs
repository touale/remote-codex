use super::policy;
use crate::{Result, config::Runtime, storage::Store};
use remote_codex_adapter::executor::Executor;
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub(super) struct Pending {
    original: Value,
    job: Option<String>,
}

pub(super) async fn dispatch(
    identity: (&str, &str),
    runtime: &Runtime,
    store: &Store,
    backend: &mut Executor,
    pending: &mut HashMap<String, Pending>,
    authorized: (
        &str,
        Option<&remote_codex_protocol::CommandApproval>,
        Option<&remote_codex_protocol::SessionPermissions>,
    ),
    mut message: Value,
) -> Result<()> {
    let (channel, profile) = identity;
    let (operation, approval, permissions) = authorized;
    let original = message.get("id").cloned();
    let key = format!("{channel}/{operation}");
    let digest = format!(
        "{:x}",
        Sha256::digest(
            json!({"message":message,"approval":approval,"permissions":permissions}).to_string()
        )
    );
    if let Some(existing) = store.operation_state(&key, &digest).await? {
        if let Some(response) = existing
            && !response.is_null()
        {
            store.append_event(channel, &response).await?;
        }
        receipt(store, channel, operation).await?;
        return Ok(());
    }
    if pending.len() >= 64 {
        return Err(Fault::new(
            "EXECUTION_BUSY",
            "too many pending execution requests",
        ));
    }
    store.begin_operation(&key, &digest).await?;
    let permission = match (approval, permissions) {
        (Some(_), Some(_)) => Err(Fault::new(
            "APPROVAL_MISMATCH",
            "multiple execution authorization scopes",
        )),
        (_, Some(grant)) if !grant.matches(channel, &message) => Err(Fault::new(
            "APPROVAL_MISMATCH",
            "session permissions do not authorize this operation",
        )),
        (Some(grant), _) if !grant.valid_for(channel, operation, &message) => Err(Fault::new(
            "APPROVAL_MISMATCH",
            "approval does not authorize this operation",
        )),
        _ => policy::validate(runtime, &message, approval.is_some(), permissions.is_some()),
    };
    if let Err(error) = permission {
        let response = json!({"id":original,"error":{"code":-32000,"message":error.to_string()}});
        store.complete_operation(&key, &response).await?;
        store.append_event(channel, &response).await?;
        receipt(store, channel, operation).await?;
        return Ok(());
    }
    if let Some(approval) = approval {
        store.record_approval(approval).await?;
    }
    if let Some(permissions) = permissions {
        store.record_permissions(permissions).await?;
    }
    if message.get("method").is_none() {
        backend.send(&message).await?;
        store.complete_operation(&key, &Value::Null).await?;
        receipt(store, channel, operation).await?;
        return Ok(());
    }
    let job = if message["method"] == "process/start" {
        let kind = if policy::authorized_mcp(runtime, &message["params"]) {
            "mcp"
        } else {
            "command"
        };
        Some(
            store
                .record_job(profile, channel, &message["params"], kind)
                .await?,
        )
    } else {
        None
    };
    if let Some(original) = original {
        message["id"] = Value::String(operation.into());
        pending.insert(operation.into(), Pending { original, job });
    }
    backend.send(&message).await?;
    if message.get("id").is_none() {
        store.complete_operation(&key, &Value::Null).await?;
    }
    receipt(store, channel, operation).await?;
    Ok(())
}

async fn receipt(store: &Store, channel: &str, operation: &str) -> Result<()> {
    store
        .append_event(
            channel,
            &json!({"method":"remoteCodex/operationAccepted","params":{"operation":operation}}),
        )
        .await?;
    Ok(())
}

pub(super) async fn collect(
    channel: &str,
    store: &Store,
    pending: &mut HashMap<String, Pending>,
    mut value: Value,
) -> Result<()> {
    let mut completed = None;
    if value.get("method").is_none() {
        if let Some(operation) = value["id"].as_str().map(str::to_owned)
            && let Some(request) = pending.remove(&operation)
        {
            completed = Some(format!("{channel}/{operation}"));
            value["id"] = request.original;
            if let Some(job) = request.job {
                store
                    .set_job_state(
                        &job,
                        if value.get("error").is_some() {
                            "failed"
                        } else {
                            "running"
                        },
                        None,
                    )
                    .await?;
            }
            store
                .complete_operation(&format!("{channel}/{operation}"), &value)
                .await?;
        } else {
            return Ok(());
        }
    } else {
        match value["method"].as_str() {
            Some("process/output") => store.record_output(channel, &value["params"]).await?,
            Some("process/exited") => {
                if let Some(id) = value.pointer("/params/processId").and_then(Value::as_str) {
                    let exit = value.pointer("/params/exitCode").and_then(Value::as_i64);
                    if let Some(id) = store.process_job(channel, id).await? {
                        store
                            .set_job_state(
                                &id,
                                if exit == Some(0) {
                                    "completed"
                                } else {
                                    "failed"
                                },
                                exit,
                            )
                            .await?;
                    }
                }
            }
            _ => {}
        }
    }
    let cursor = store.append_event(channel, &value).await?;
    if let Some(key) = completed {
        store.link_operation(&key, cursor).await?;
    }
    Ok(())
}
