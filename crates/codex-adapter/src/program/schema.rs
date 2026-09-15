use remote_codex_protocol::Fault;
use serde_json::Value;
use std::{path::Path, process::Stdio, time::Duration};

// Contracts the adapter actually sends, not a list of accepted release numbers.
const REQUESTS: &[(&str, &[&str])] = &[
    ("initialize", &["clientInfo", "capabilities"]),
    ("environment/add", &["environmentId", "execServerUrl"]),
    (
        "thread/start",
        &["environments", "dynamicTools", "sandbox", "approvalPolicy"],
    ),
    ("thread/resume", &["threadId", "excludeTurns"]),
    ("thread/fork", &["threadId", "beforeTurnId", "excludeTurns"]),
    ("thread/settings/update", &["threadId"]),
    ("thread/turns/list", &["threadId"]),
    ("thread/items/list", &["threadId"]),
    (
        "turn/start",
        &["threadId", "input", "environments", "runtimeWorkspaceRoots"],
    ),
    ("turn/interrupt", &["threadId", "turnId"]),
    ("thread/goal/get", &["threadId"]),
    ("thread/goal/set", &["threadId"]),
    ("thread/goal/clear", &["threadId"]),
    ("skills/list", &["cwds"]),
    ("mcpServerStatus/list", &[]),
    ("config/batchWrite", &["edits"]),
];

pub(super) async fn inspect(program: &Path, home: &Path) -> Result<(), Fault> {
    let output = home.join("schema");
    let status = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(program)
            .args([
                "app-server",
                "generate-json-schema",
                "--experimental",
                "--out",
            ])
            .arg(&output)
            .env("CODEX_HOME", home)
            .current_dir(home)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    .map_err(|_| incompatible("protocol inspection timed out"))?
    .map_err(|_| incompatible("cannot inspect the native protocol"))?;
    if !status.success() {
        return Err(incompatible("experimental protocol schema is unavailable"));
    }
    let path = output.join("ClientRequest.json");
    if std::fs::metadata(&path)
        .map_err(|_| incompatible("request schema is missing"))?
        .len()
        > 16 * 1024 * 1024
    {
        return Err(incompatible("request schema exceeds the inspection limit"));
    }
    let schema: Value = serde_json::from_slice(
        &std::fs::read(path).map_err(|_| incompatible("cannot read request schema"))?,
    )
    .map_err(|_| incompatible("invalid request schema"))?;
    validate(&schema)
}

fn validate(schema: &Value) -> Result<(), Fault> {
    let variants = schema["oneOf"]
        .as_array()
        .ok_or_else(|| incompatible("request methods are missing"))?;
    for (method, fields) in REQUESTS {
        let request = variants
            .iter()
            .find(|v| {
                v.pointer("/properties/method/enum")
                    .and_then(Value::as_array)
                    .is_some_and(|names| names.iter().any(|v| v == method))
            })
            .ok_or_else(|| incompatible(&format!("missing method {method}")))?;
        let params = &request["properties"]["params"];
        let params = match params["$ref"].as_str().and_then(|r| r.strip_prefix('#')) {
            Some(pointer) => schema
                .pointer(pointer)
                .ok_or_else(|| incompatible("unresolved parameter schema"))?,
            None => params,
        };
        for field in *fields {
            if params["properties"].get(field).is_none() {
                return Err(incompatible(&format!("missing field {method}.{field}")));
            }
        }
    }
    Ok(())
}

fn incompatible(detail: &str) -> Fault {
    Fault::new(
        "UNSUPPORTED_CODEX_PROTOCOL",
        &format!("local Codex is incompatible with this adapter: {detail}"),
    )
}
