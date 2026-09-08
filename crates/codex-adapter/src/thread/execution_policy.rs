use remote_codex_core::session::SessionBinding;
use remote_codex_protocol::Fault;
use serde_json::Value;
use std::path::{Component, Path};

/// The native thread owns preferences; execution checks require current authority.
pub(crate) fn validate(
    params: &Value,
    binding: &SessionBinding,
    full_access: bool,
) -> Result<(), Fault> {
    if let Some(cwd) = params.get("cwd").filter(|value| !value.is_null())
        && cwd.as_str() != Some(&binding.session.cwd)
    {
        return Err(Fault::new(
            "WORKSPACE_SETTINGS_LOCKED",
            "the session's execution directory cannot change",
        ));
    }
    let unrestricted = binding.execution_mode == "unrestricted" || full_access;
    let permissions = params.get("permissions").filter(|value| !value.is_null());
    let sandbox = params.get("sandboxPolicy").filter(|value| !value.is_null());
    if permissions.is_some() && sandbox.is_some() {
        return Err(Fault::new(
            "INVALID_NATIVE_SETTINGS",
            "choose a permissions preset or a sandbox policy, not both",
        ));
    }
    if let Some(permissions) = permissions {
        match permissions.as_str() {
            Some(":workspace") => {}
            Some(":danger-full-access") if unrestricted => {}
            Some(":danger-full-access") => return Err(exceeds_limit()),
            _ => {
                return Err(Fault::new(
                    "UNSUPPORTED_PERMISSION_PRESET",
                    "this workspace supports the native workspace and Full Access presets",
                ));
            }
        }
    }
    if let Some(policy) = sandbox {
        match policy["type"].as_str() {
            Some("dangerFullAccess") if unrestricted => {}
            Some("readOnly" | "workspaceWrite") => {
                if !unrestricted && policy["networkAccess"] == true {
                    return Err(exceeds_limit());
                }
                if let Some(roots) = policy.get("writableRoots") {
                    for root in roots.as_array().ok_or_else(exceeds_limit)? {
                        let path = Path::new(root.as_str().ok_or_else(exceeds_limit)?);
                        if !unrestricted
                            && (!path.starts_with(&binding.session.cwd)
                                || path.components().any(|c| matches!(c, Component::ParentDir)))
                        {
                            return Err(exceeds_limit());
                        }
                    }
                }
            }
            _ => return Err(exceeds_limit()),
        }
    }
    if let Some(reviewer) = params.get("approvalsReviewer") {
        validate_reviewer(reviewer)?;
    }
    Ok(())
}

pub(crate) fn validate_reviewer(value: &Value) -> Result<(), Fault> {
    if value.is_null()
        || matches!(
            value.as_str(),
            Some("user" | "auto_review" | "guardian_subagent")
        )
    {
        Ok(())
    } else {
        Err(Fault::new(
            "INVALID_APPROVALS_REVIEWER",
            "unsupported native approval reviewer",
        ))
    }
}

fn exceeds_limit() -> Fault {
    Fault::new(
        "EXECUTION_PERMISSION_LIMIT",
        "these permissions require an explicit Full Access choice for this session",
    )
}
