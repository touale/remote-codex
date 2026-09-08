use super::execution_policy;
use remote_codex_core::session::SessionBinding;
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::path::Path;

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;

/// Native preferences may change while the environment's authority stays fixed.
pub(crate) fn prepare_thread(
    method: &str,
    params: &Value,
    binding: &SessionBinding,
) -> Result<(), Fault> {
    let running = method == "turn/settings/update";
    let fields = if running {
        &[
            "threadId",
            "turnId",
            "model",
            "effort",
            "summary",
            "serviceTier",
            "approvalsReviewer",
        ][..]
    } else {
        &[
            "threadId",
            "model",
            "effort",
            "summary",
            "serviceTier",
            "personality",
            "collaborationMode",
            "multiAgentMode",
            "cwd",
            "permissions",
            "sandboxPolicy",
            "approvalPolicy",
            "approvalsReviewer",
        ][..]
    };
    known_fields(params, fields)?;
    if params["threadId"].as_str() != Some(&binding.session.id) {
        return Err(Fault::new(
            "SESSION_MISMATCH",
            "settings belong to another session",
        ));
    }
    // This method is a user preference action from the private native frontend.
    // Execution is authorized separately after native confirmation succeeds.
    let full = params["permissions"] == ":danger-full-access"
        || params
            .pointer("/sandboxPolicy/type")
            .and_then(Value::as_str)
            == Some("dangerFullAccess");
    execution_policy::validate(params, binding, full)
}

/// Persist native preferences only in the local Codex user configuration.
/// Validate the complete batch before forwarding any write to native Codex.
pub(crate) fn prepare_config(method: &str, params: &mut Value, home: &Path) -> Result<(), Fault> {
    let batch = method == "config/batchWrite";
    known_fields(
        params,
        if batch {
            &["edits", "filePath", "expectedVersion", "reloadUserConfig"]
        } else {
            &[
                "keyPath",
                "value",
                "mergeStrategy",
                "filePath",
                "expectedVersion",
            ]
        },
    )?;
    let target = home.join("config.toml");
    if let Some(path) = params.get("filePath").filter(|value| !value.is_null()) {
        let path = path.as_str().ok_or_else(invalid)?;
        if Path::new(path) != target
            && target.canonicalize().ok().as_deref() != Some(Path::new(path))
        {
            return Err(Fault::new(
                "CONFIG_TARGET_MISMATCH",
                "defaults must be saved to this computer's Codex user config",
            ));
        }
    }
    if let Some(version) = params.get("expectedVersion")
        && !version.is_null()
        && !version.is_string()
    {
        return Err(invalid());
    }
    if batch {
        if params
            .get("reloadUserConfig")
            .is_some_and(|value| !value.is_boolean())
        {
            return Err(invalid());
        }
        let edits = params["edits"].as_array().ok_or_else(invalid)?;
        if edits.is_empty() || edits.len() > 64 {
            return Err(invalid());
        }
        for edit in edits {
            known_fields(edit, &["keyPath", "value", "mergeStrategy"])?;
            validate_edit(edit)?;
        }
    } else {
        validate_edit(params)?;
    }
    params["filePath"] = json!(target.to_str().ok_or_else(invalid)?);
    if batch {
        // Apply live preferences through thread/turn settings; saving a default
        // must not reload unrelated permission or execution configuration.
        params["reloadUserConfig"] = json!(false);
    }
    Ok(())
}

fn validate_edit(edit: &Value) -> Result<(), Fault> {
    let key = edit["keyPath"].as_str().ok_or_else(invalid)?;
    if !preference_key(key) {
        return Err(Fault::new(
            "UNSUPPORTED_CONFIG_SETTING",
            "this workspace can save native model and reviewer preferences; the execution ceiling is managed by remote-codex config -n NAME",
        ));
    }
    if !matches!(edit["mergeStrategy"].as_str(), Some("replace" | "upsert"))
        || edit
            .get("value")
            .is_none_or(|value| !value.is_null() && !value.is_string())
    {
        return Err(invalid());
    }
    if key.rsplit('.').next() == Some("approvals_reviewer") {
        execution_policy::validate_reviewer(&edit["value"])?;
    }
    Ok(())
}

fn preference_key(key: &str) -> bool {
    const KEYS: &[&str] = &[
        "model",
        "model_reasoning_effort",
        "model_reasoning_summary",
        "model_verbosity",
        "plan_mode_reasoning_effort",
        "service_tier",
        "personality",
        "review_model",
        "approvals_reviewer",
    ];
    if KEYS.contains(&key) {
        return true;
    }
    let Some((profile, key)) = key
        .strip_prefix("profiles.")
        .and_then(|key| key.split_once('.'))
    else {
        return false;
    };
    !profile.is_empty()
        && profile
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        && KEYS.contains(&key)
}

fn known_fields(value: &Value, allowed: &[&str]) -> Result<(), Fault> {
    let object = value.as_object().ok_or_else(invalid)?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> Fault {
    Fault::new(
        "INVALID_NATIVE_SETTINGS",
        "unsupported or invalid native settings parameters",
    )
}
