use remote_codex_adapter::{events::public_event, status};
use remote_codex_core::session::SessionEvent;
use serde_json::json;
#[test]
fn native_turn_message_and_usage_metadata_survives_projection()
-> Result<(), Box<dyn std::error::Error>> {
    let event = json!({"method":"turn/completed","params":{"turn":{"id":"turn","status":"completed","startedAt":100,"completedAt":120,"durationMs":19750}}});
    let Some(SessionEvent::TurnCompleted { timing, .. }) = public_event(&event) else {
        return Err("turn projection missing".into());
    };
    assert_eq!(timing.started_at, Some(100));
    assert_eq!(timing.completed_at, Some(120));
    assert_eq!(timing.duration_ms, Some(19750));
    assert!(status::timing(&json!({})).started_at.is_none());
    let event = json!({"method":"item/completed","params":{"turnId":"turn","item":{"type":"userMessage","id":"native","clientId":"client","content":[{"type":"text","text":"Follow up"}]}}});
    let Some(SessionEvent::UserMessage {
        client_id,
        turn_id,
        text,
        ..
    }) = public_event(&event)
    else {
        return Err("user projection missing".into());
    };
    assert_eq!(client_id.as_deref(), Some("client"));
    assert_eq!(turn_id, "turn");
    assert_eq!(text, "Follow up");
    let event = json!({"method":"thread/tokenUsage/updated","params":{"tokenUsage":{"last":{"totalTokens":500,"inputTokens":400,"cachedInputTokens":100,"outputTokens":100,"reasoningOutputTokens":50},"total":{"totalTokens":20000},"modelContextWindow":128000}}});
    let Some(SessionEvent::UsageChanged { usage }) = public_event(&event) else {
        return Err("usage projection missing".into());
    };
    assert_eq!(usage.last_tokens, 500);
    assert_eq!(usage.total_tokens, 20000);
    assert_eq!(usage.context_window, Some(128000));
    let values = status::limits(
        &json!({"rateLimits":{"primary":{"usedPercent":25,"windowDurationMins":300,"resetsAt":1000}}}),
    );
    assert_eq!(
        values[0]
            .primary
            .as_ref()
            .ok_or("rate window missing")?
            .resets_at,
        Some(1000)
    );
    assert!(status::limits(&json!({})).is_empty());
    let main = json!({"limitId":"codex", "primary":{"usedPercent":40,"windowDurationMins":10080},"secondary":{"usedPercent":12,"windowDurationMins":300}});
    let projected = status::limits(
        &json!({"rateLimits":main,"rateLimitsByLimitId":{"extra":{"limitName":"Extra"},"codex":main}}),
    );
    let primary = projected
        .iter()
        .find(|limit| limit.is_default)
        .ok_or("primary quota missing")?;
    assert_eq!(primary.id.as_deref(), Some("codex"));
    assert_eq!(
        primary
            .secondary
            .as_ref()
            .ok_or("five-hour quota missing")?
            .window_minutes,
        Some(300)
    );
    assert!(
        !projected
            .iter()
            .find(|limit| limit.id.as_deref() == Some("extra"))
            .ok_or("extra quota missing")?
            .is_default
    );
    Ok(())
}
