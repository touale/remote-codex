use crate::{Result, config::Runtime};
use remote_codex_protocol::Fault;
use serde_json::Value;

pub(super) fn validate(
    runtime: &Runtime,
    message: &Value,
    approved_command: bool,
    full_access: bool,
) -> Result<()> {
    let Some(method) = message["method"].as_str() else {
        return if message.get("id").is_some()
            && (message.get("result").is_some() || message.get("error").is_some())
        {
            Ok(())
        } else {
            Err(Fault::new(
                "INVALID_EXECUTION",
                "invalid execution response",
            ))
        };
    };
    if !matches!(
        method,
        "initialize"
            | "initialized"
            | "environment/info"
            | "environment/status"
            | "environmentConfig/read"
            | "capabilityRoots/discoverV1"
    ) && !["process/", "fs/", "http/", "network/", "shell/"]
        .iter()
        .any(|prefix| method.starts_with(prefix))
    {
        return Err(Fault::new(
            "EXECUTION_METHOD_DENIED",
            "only execution environment methods are permitted",
        ));
    }
    if matches!(
        method,
        "fs/writeFile" | "fs/createDirectory" | "fs/remove" | "fs/copy"
    ) && !runtime.unrestricted
        && !full_access
        && !managed(&message["params"]["sandbox"])
    {
        return Err(Fault::new(
            "SANDBOX_REQUIRED",
            "filesystem mutation requires the configured sandbox",
        ));
    }
    if method == "process/start" {
        let params = &message["params"];
        if !runtime.unrestricted
            && !full_access
            && !managed(&params["sandbox"])
            && !authorized_mcp(runtime, params)
            && !approved_command
        {
            return Err(Fault::new(
                "SANDBOX_REQUIRED",
                "this environment requires sandboxed execution or an explicit one-time command approval",
            ));
        }
        if let Some(env) = params["env"].as_object() {
            for key in [
                "OPENAI_API_KEY",
                "CODEX_API_KEY",
                "CODEX_ACCESS_TOKEN",
                "CODEX_AUTH_JSON",
            ] {
                if env.contains_key(key) {
                    return Err(Fault::new(
                        "CREDENTIAL_FORWARDING_DENIED",
                        "Codex account credentials cannot be forwarded to the execution environment",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn managed(sandbox: &Value) -> bool {
    sandbox.pointer("/permissions/type").and_then(Value::as_str) == Some("managed")
        && sandbox
            .pointer("/permissions/file_system/type")
            .and_then(Value::as_str)
            == Some("restricted")
}

pub(super) fn authorized_mcp(runtime: &Runtime, params: &Value) -> bool {
    let Some(argv) = params["argv"].as_array() else {
        return false;
    };
    let cwd = params["cwd"]
        .as_str()
        .and_then(|s| url::Url::parse(s).ok())
        .and_then(|u| u.to_file_path().ok());
    runtime.mcp.iter().any(|allowed| {
        cwd.as_deref() == Some(std::path::Path::new(&allowed.cwd))
            && argv.len() == allowed.argv.len()
            && argv
                .iter()
                .zip(&allowed.argv)
                .all(|(actual, expected)| actual.as_str() == Some(expected))
    })
}
