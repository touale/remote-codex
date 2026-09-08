//! Native notification parsing is confined to the versioned adapter.
use remote_codex_core::session::{
    ApprovalDecision, ApprovalsReviewer, PermissionPreset, SessionEvent, SessionSettings,
    TurnOutcome,
};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};

pub struct Approval {
    pub request_id: String,
    pub thread: String,
    pub environment: String,
    pub turn: String,
    pub item: String,
    pub argv: Vec<String>,
    pub cwd: String,
}

pub enum Notice {
    Approval(Approval),
    Completed {
        item: Option<String>,
        turn: Option<String>,
    },
    Permissions {
        thread: String,
        full_access: bool,
    },
    Closed(Option<Fault>),
    Other,
}

pub fn notice(event: &mut Value, restrict_commands: bool) -> Result<Notice, Fault> {
    let text = |pointer: &str| {
        event
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    match event["method"].as_str() {
        Some("item/completed" | "turn/completed") => Ok(Notice::Completed {
            item: text("/params/item/id"),
            turn: text("/params/turn/id"),
        }),
        Some("thread/settings/updated") => Ok(Notice::Permissions {
            thread: text("/params/threadId").ok_or_else(invalid)?,
            full_access: event
                .pointer("/params/threadSettings/sandboxPolicy/type")
                .and_then(Value::as_str)
                == Some("dangerFullAccess"),
        }),
        Some("remoteCodex/engineClosed") => Ok(Notice::Closed(
            event
                .pointer("/params/fault")
                .filter(|v| !v.is_null())
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|_| invalid())?,
        )),
        Some("item/commandExecution/requestApproval") if restrict_commands => {
            if event
                .pointer("/params/kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind != "command")
            {
                return Ok(Notice::Other);
            }
            let approval = Approval {
                request_id: event.get("id").ok_or_else(invalid)?.to_string(),
                thread: text("/params/threadId").ok_or_else(invalid)?,
                environment: text("/params/environmentId").ok_or_else(invalid)?,
                turn: text("/params/turnId").ok_or_else(invalid)?,
                item: text("/params/itemId").ok_or_else(invalid)?,
                argv: shlex::split(&text("/params/command").ok_or_else(invalid)?)
                    .filter(|v| !v.is_empty())
                    .ok_or_else(invalid)?,
                cwd: url::Url::from_file_path(text("/params/cwd").ok_or_else(invalid)?)
                    .map_err(|_| invalid())?
                    .to_string(),
            };
            event["params"]["availableDecisions"] = json!(["accept", "cancel"]);
            event["params"]["proposedExecpolicyAmendment"] = Value::Null;
            Ok(Notice::Approval(approval))
        }
        _ => Ok(Notice::Other),
    }
}

pub fn public_event(event: &Value) -> Option<SessionEvent> {
    let text = |pointer| {
        event
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    if let (Some(id), Some(method)) = (event.get("id"), event["method"].as_str()) {
        return Some(
            if matches!(
                method,
                "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
            ) {
                SessionEvent::ApprovalRequested {
                    request_id: id.to_string(),
                    description: event
                        .pointer("/params/command")
                        .or_else(|| event.pointer("/params/reason"))
                        .and_then(Value::as_str)
                        .unwrap_or("Codex requests approval")
                        .into(),
                }
            } else {
                SessionEvent::InteractionRequired {
                    request_id: id.to_string(),
                    kind: method.into(),
                }
            },
        );
    }
    match event["method"].as_str()? {
        "item/agentMessage/delta" => Some(SessionEvent::Message {
            item_id: text("/params/itemId"),
            text: text("/params/delta"),
            complete: false,
        }),
        "item/completed"
            if event.pointer("/params/item/type").and_then(Value::as_str)
                == Some("agentMessage") =>
        {
            Some(SessionEvent::Message {
                item_id: text("/params/item/id"),
                text: text("/params/item/text"),
                complete: true,
            })
        }
        "item/commandExecution/outputDelta" => Some(SessionEvent::ToolOutput {
            item_id: text("/params/itemId"),
            text: text("/params/delta"),
        }),
        "turn/started" => Some(SessionEvent::TurnStarted {
            id: text("/params/turn/id"),
        }),
        "turn/completed" => Some(SessionEvent::TurnCompleted {
            id: text("/params/turn/id"),
            outcome: match event.pointer("/params/turn/status").and_then(Value::as_str) {
                Some("completed") => TurnOutcome::Completed,
                Some("interrupted") => TurnOutcome::Interrupted,
                _ => TurnOutcome::Failed {
                    message: event
                        .pointer("/params/turn/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("native turn failed")
                        .into(),
                },
            },
        }),
        "thread/settings/updated" => Some(SessionEvent::PermissionsUpdated {
            full_access: event
                .pointer("/params/threadSettings/sandboxPolicy/type")
                .and_then(Value::as_str)
                == Some("dangerFullAccess"),
        }),
        _ => None,
    }
}

pub fn approval_response(id: &str, decision: ApprovalDecision) -> Result<Value, Fault> {
    let id: Value = serde_json::from_str(id).map_err(|_| invalid())?;
    Ok(
        json!({"id":id,"result":{"decision":match decision {ApprovalDecision::AcceptOnce=>"accept",ApprovalDecision::Cancel=>"cancel"}}}),
    )
}

pub fn decision(response: &Value) -> Result<bool, Fault> {
    match response.pointer("/result/decision").and_then(Value::as_str) {
        Some("accept") => Ok(true),
        Some("cancel" | "decline") | None => Ok(false),
        _ => Err(Fault::new(
            "UNSUPPORTED_APPROVAL_SCOPE",
            "approve this command once or cancel it",
        )),
    }
}

pub fn settings(thread: &str, options: SessionSettings) -> Value {
    let mut value = json!({"threadId":thread});
    if let Some(model) = options.model {
        value["model"] = json!(model);
    }
    if let Some(effort) = options.effort {
        value["effort"] = json!(effort);
    }
    if let Some(preset) = options.permissions {
        value["permissions"] = json!(match preset {
            PermissionPreset::Workspace => ":workspace",
            PermissionPreset::FullAccess => ":danger-full-access",
        });
    }
    if let Some(reviewer) = options.reviewer {
        value["approvalsReviewer"] = json!(match reviewer {
            ApprovalsReviewer::User => "user",
            ApprovalsReviewer::AutoReview => "auto_review",
            ApprovalsReviewer::GuardianSubagent => "guardian_subagent",
        });
    }
    value
}

pub fn prompt(thread: &str, text: &str) -> Value {
    json!({"threadId":thread,"input":[{"type":"text","text":text}]})
}
pub fn interrupt(thread: &str, turn: &str) -> Value {
    json!({"threadId":thread,"turnId":turn})
}
fn invalid() -> Fault {
    Fault::new(
        "INVALID_NATIVE_EVENT",
        "Codex emitted an invalid session event",
    )
}
