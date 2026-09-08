use serde_json::{Value, json};

pub struct RecoveryRequest {
    pub rpc_id: Value,
    pub thread: String,
    pub job: Option<String>,
    pub after: i64,
}

pub fn tools() -> Value {
    json!([{"type":"function","name":"remote_jobs","description":"Inspect commands previously submitted by this local thread to its bound remote environment. Use after reconnection; never rerun a command solely because its terminal ID is unavailable.","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"after":{"type":"integer","minimum":0}},"additionalProperties":false}}])
}

pub fn request(event: &Value) -> Option<RecoveryRequest> {
    if event["method"] != "item/tool/call"
        || event.pointer("/params/tool").and_then(Value::as_str) != Some("remote_jobs")
    {
        return None;
    }
    Some(RecoveryRequest {
        rpc_id: event["id"].clone(),
        thread: event
            .pointer("/params/threadId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        job: event
            .pointer("/params/arguments/id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        after: event
            .pointer("/params/arguments/after")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    })
}

pub fn response(id: Value, result: Result<Value, String>) -> Value {
    let (success, text) = match result {
        Ok(value) => (true, value.to_string()),
        Err(error) => (false, error),
    };
    json!({"id":id,"result":{"success":success,"contentItems":[{"type":"inputText","text":text}]}})
}
