use super::*;
use remote_codex_protocol::Request;

pub(super) fn tools() -> Value {
    json!([{"type":"function","name":"remote_jobs","description":"Inspect commands previously submitted by this local thread to its bound remote environment. Use after reconnection; never rerun a command solely because its terminal ID is unavailable.",
        "inputSchema":{"type":"object","properties":{"id":{"type":"string"},"after":{"type":"integer","minimum":0}},"additionalProperties":false}}])
}

pub(super) async fn handle(runtime: &LocalRuntime, event: &Value) -> Option<Value> {
    if event["method"] != "item/tool/call"
        || event.pointer("/params/tool").and_then(Value::as_str) != Some("remote_jobs")
    {
        return None;
    }
    let result = async {
        if event.pointer("/params/threadId").and_then(Value::as_str)
            != Some(&runtime.binding.session.id)
        {
            return Err(ClientError::Argument("job query belongs to another thread"));
        }
        let jobs = runtime
            .remote
            .call(Request::Jobs {
                thread: Some(runtime.binding.session.id.clone()),
            })
            .await?;
        if let Some(id) = event
            .pointer("/params/arguments/id")
            .and_then(Value::as_str)
        {
            if !jobs
                .as_array()
                .is_some_and(|jobs| jobs.iter().any(|job| job["id"] == id))
            {
                return Err(ClientError::NotFound);
            }
            runtime
                .remote
                .call(Request::JobOutput {
                    id: id.into(),
                    after: event
                        .pointer("/params/arguments/after")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                })
                .await
        } else {
            Ok(jobs)
        }
    }
    .await;
    let (success, text) = match result {
        Ok(value) => (true, value.to_string()),
        Err(error) => (false, error.to_string()),
    };
    Some(
        json!({"id":event["id"],"result":{"success":success,"contentItems":[{"type":"inputText","text":text}]}}),
    )
}
