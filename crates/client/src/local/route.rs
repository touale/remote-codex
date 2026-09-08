use super::{LocalRuntime, binding, execution_policy, settings};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

pub(super) async fn request(
    runtime: &LocalRuntime,
    method: &str,
    mut params: Value,
) -> Result<Value, Fault> {
    if *runtime.lease.revoked.borrow() {
        return Err(Fault::new(
            "LEASE_REVOKED",
            "session control moved to another frontend",
        ));
    }
    if method == "initialize" {
        return Ok(runtime.initialized.clone());
    }
    if matches!(method, "thread/list" | "thread/loaded/list") {
        return Ok(if method == "thread/list" {
            json!({"data":[],"nextCursor":null})
        } else {
            json!({"data":[runtime.binding.session.id],"nextCursor":null})
        });
    }
    if (method.starts_with("thread/") || method.starts_with("turn/"))
        && params["threadId"].as_str() != Some(&runtime.binding.session.id)
    {
        return Err(Fault::new(
            "SESSION_MISMATCH",
            "use remote-codex to select or create another session",
        ));
    }
    if method == "thread/unsubscribe" {
        return Ok(json!({"status":"unsubscribed"}));
    }
    if matches!(method, "thread/settings/update" | "turn/settings/update") {
        settings::prepare_thread(method, &params, &runtime.binding)?;
        let ticket = runtime.permissions.begin(
            &params,
            runtime
                .remote
                .identity
                .capabilities
                .iter()
                .any(|c| c == remote_codex_protocol::SESSION_PERMISSIONS_CAPABILITY),
        )?;
        let result = runtime.engine.call(method, params).await;
        if let Some(ticket) = ticket {
            runtime.permissions.acknowledge(&ticket, result.is_ok())?;
            if result.is_ok() {
                runtime.permissions.confirmed(&ticket).await?;
            }
        }
        return result;
    }
    if matches!(method, "config/batchWrite" | "config/value/write") {
        settings::prepare_config(
            method,
            &mut params,
            std::path::Path::new(&runtime.binding.codex_home),
        )?;
        return runtime.engine.call(method, params).await;
    }
    if method == "thread/resume" {
        let mut snapshot = runtime.bootstrap.clone();
        let current = runtime
            .engine
            .call(
                "thread/read",
                json!({"threadId":runtime.binding.session.id,"includeTurns":false}),
            )
            .await?;
        snapshot["thread"] = current["thread"].clone();
        return Ok(snapshot);
    }
    if matches!(method, "thread/turns/list" | "thread/items/list")
        && !runtime.has_history.load(Ordering::Acquire)
    {
        return Ok(json!({"data":[],"nextCursor":null,"backwardsCursor":null}));
    }
    if method == "turn/start" {
        runtime.bridge.check().map_err(|_| {
            Fault::unknown(
                "execution environment is unavailable; reopen this session before continuing",
            )
        })?;
        binding::turn_params(&mut params, &runtime.binding);
        execution_policy::validate(&params, &runtime.binding, runtime.permissions.full_access())?;
        runtime.skills.remap_inputs(&mut params);
        // Native Codex does not persist empty threads. Save the binding before
        // submitting the first turn, so a crashed UI can still recover its work.
        if !runtime.has_history.load(Ordering::Acquire) {
            runtime
                .store
                .save_session(&runtime.binding)
                .await
                .map_err(|_| {
                    Fault::new(
                        "SESSION_STORAGE",
                        "could not persist the local session binding; no turn was submitted",
                    )
                })?;
            runtime.has_history.store(true, Ordering::Release);
        }
    }
    if method == "turn/steer" {
        params["cwd"] = json!(runtime.binding.session.cwd);
    }
    const ALLOWED: &[&str] = &[
        "thread/read",
        "thread/turns/list",
        "thread/items/list",
        "thread/goal/get",
        "thread/name/set",
        "turn/start",
        "turn/steer",
        "turn/interrupt",
        "model/list",
        "account/read",
        "account/login/start",
        "account/login/cancel",
        "account/rateLimits/read",
        "config/read",
        "configRequirements/read",
        "skills/list",
        "plugin/list",
        "hooks/list",
        "mcpServerStatus/list",
    ];
    if !ALLOWED.contains(&method) {
        return Err(Fault::new(
            "UNSUPPORTED_CODEX_METHOD",
            "this native operation is not validated for a bound remote workspace",
        ));
    }
    runtime.engine.call(method, params).await
}

pub(super) fn response(id: Value, result: Result<Value, Fault>) -> Value {
    match result {
        Ok(value) => json!({"id":id,"result":value}),
        Err(error) => {
            json!({"id":id,"error":{"code":-32000,"message":error.message,"data":{"code":error.code,"outcome_unknown":error.outcome_unknown}}})
        }
    }
}
