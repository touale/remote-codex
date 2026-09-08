use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const COMMAND_APPROVAL_CAPABILITY: &str = "command-approval-v1";
pub const SESSION_PERMISSIONS_CAPABILITY: &str = "session-permissions-v1";

/// A frontend-confirmed Full Access choice for one thread and execution channel.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPermissions {
    pub id: String,
    pub channel: String,
    pub thread: String,
}

impl SessionPermissions {
    pub fn matches(&self, channel: &str, message: &Value) -> bool {
        if self.id.is_empty() || self.thread.is_empty() || self.channel != channel {
            return false;
        }
        match message["method"].as_str() {
            Some("process/start") => {
                message
                    .pointer("/params/env/CODEX_THREAD_ID")
                    .and_then(Value::as_str)
                    == Some(&self.thread)
            }
            Some("fs/writeFile" | "fs/createDirectory" | "fs/remove" | "fs/copy") => true,
            _ => false,
        }
    }
}

/// Issued only by the trusted local client after an explicit native approval.
/// Never deserialize authority from a native execution message itself.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandApproval {
    pub id: String,
    pub channel: String,
    pub thread: String,
    pub turn: String,
    pub item: String,
    pub argv: Vec<String>,
    pub cwd: String,
}

impl CommandApproval {
    pub fn matches(&self, message: &Value) -> bool {
        message["method"] == "process/start"
            && message["params"]["argv"] == serde_json::json!(self.argv)
            && message["params"]["cwd"].as_str() == Some(&self.cwd)
            && message["params"]["arg0"].is_null()
            && message
                .pointer("/params/env/CODEX_THREAD_ID")
                .and_then(Value::as_str)
                == Some(&self.thread)
    }

    pub fn valid_for(&self, channel: &str, operation: &str, message: &Value) -> bool {
        self.channel == channel
            && self.id == operation
            && !self.thread.is_empty()
            && !self.turn.is_empty()
            && !self.item.is_empty()
            && self.matches(message)
    }
}
