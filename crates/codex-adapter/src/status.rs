//! Native status and timing projection, shared by history and live events.
use crate::thread::Thread;
use remote_codex_core::{session::SessionEvent, status::*};
use remote_codex_protocol::Fault;
use serde_json::Value;

pub fn timing(turn: &Value) -> TurnTiming {
    TurnTiming {
        started_at: turn["startedAt"].as_i64(),
        completed_at: turn["completedAt"].as_i64(),
        duration_ms: turn["durationMs"].as_u64(),
    }
}

pub fn limits(value: &Value) -> Vec<RateLimit> {
    let main = &value["rateLimits"];
    let snapshots: Vec<(Option<&str>, &Value)> = match value["rateLimitsByLimitId"].as_object() {
        Some(map) if !map.is_empty() => map.iter().map(|(id, v)| (Some(id.as_str()), v)).collect(),
        _ => value
            .get("rateLimits")
            .filter(|v| v.is_object())
            .map(|v| (None, v))
            .into_iter()
            .collect(),
    };
    snapshots
        .into_iter()
        .map(|(key, v)| {
            let id = v["limitId"].as_str().or(key);
            RateLimit {
                id: id.map(str::to_owned),
                is_default: key.is_none()
                    || id == main["limitId"].as_str() && id.is_some()
                    || v == main,
                name: v["limitName"].as_str().or(id).unwrap_or("Codex").into(),
                primary: window(&v["primary"]),
                secondary: window(&v["secondary"]),
            }
        })
        .collect()
}

fn window(value: &Value) -> Option<RateWindow> {
    Some(RateWindow {
        used_percent: value["usedPercent"].as_f64()?,
        window_minutes: value["windowDurationMins"].as_u64(),
        resets_at: value["resetsAt"].as_i64(),
    })
}

pub fn event(value: &Value) -> Option<SessionEvent> {
    let params = &value["params"];
    Some(match value["method"].as_str()? {
        "thread/status/changed" => SessionEvent::ActivityChanged {
            activity: params["status"]["type"].as_str()?.into(),
            active_flags: flags(&params["status"]),
        },
        "thread/tokenUsage/updated" => {
            let usage = &params["tokenUsage"];
            SessionEvent::UsageChanged {
                usage: tokens(
                    &usage["last"],
                    &usage["total"],
                    &usage["modelContextWindow"],
                )?,
            }
        }
        "account/rateLimits/updated" => SessionEvent::RateLimitsChanged {
            limits: limits(params),
        },
        _ => return None,
    })
}

/// App-server notifications and durable rollout reports carry the same counters.
pub(crate) fn tokens(last: &Value, total: &Value, context: &Value) -> Option<TokenUsage> {
    let number = |value: &Value, native: &str, saved: &str| {
        value[native].as_u64().or_else(|| value[saved].as_u64())
    };
    Some(TokenUsage {
        last_tokens: number(last, "totalTokens", "total_tokens")?,
        total_tokens: number(total, "totalTokens", "total_tokens")?,
        input_tokens: number(last, "inputTokens", "input_tokens")?,
        cached_input_tokens: number(last, "cachedInputTokens", "cached_input_tokens")?,
        output_tokens: number(last, "outputTokens", "output_tokens")?,
        reasoning_tokens: number(last, "reasoningOutputTokens", "reasoning_output_tokens")?,
        context_window: context.as_u64(),
    })
}

pub fn initial(response: &Value) -> SessionStatus {
    let status = &response["thread"]["status"];
    SessionStatus {
        provider: response["modelProvider"].as_str().map(str::to_owned),
        activity: status["type"].as_str().unwrap_or("notLoaded").into(),
        active_flags: flags(status),
        ..Default::default()
    }
}

fn flags(status: &Value) -> Vec<String> {
    status["activeFlags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

impl Thread {
    pub async fn rate_limits(&self) -> Result<Vec<RateLimit>, Fault> {
        Ok(crate::usage::read(&self.codex.engine).await?.limits)
    }
}
