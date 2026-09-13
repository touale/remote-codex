mod approval;
pub mod codec;
mod files;
mod models;
mod service;
pub mod transfer;
pub use files::*;

pub use approval::*;
pub use models::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use service::*;

pub const VERSION: u32 = 4;
pub const MAX_FRAME: usize = 2 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
pub struct Call {
    pub protocol: u32,
    pub id: String,
    pub profile: String,
    pub expected_identity: Option<String>,
    pub request: Request,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    Transfer {
        workspace: String,
        expected_root: Option<transfer::RootIdentity>,
    },
    WorkspaceFiles {
        workspace: String,
        operation: FileRequest,
    },
    Hello,
    PrepareUpdate,
    Configure(RemoteConfig),
    Workspace {
        path: String,
    },
    ProjectConfig {
        path: String,
    },
    PrepareSkill {
        digest: String,
        files: Vec<SkillFile>,
    },
    CommitSkill {
        stage: String,
        digest: String,
    },
    OpenExecution {
        channel: String,
        runtime: ExecutionRuntime,
        revision: i64,
        mcp: Vec<ExecutionCommand>,
    },
    AttachExecution {
        channel: String,
        after: i64,
    },
    InspectExecution {
        channel: String,
    },
    DetachExecution {
        channel: String,
    },
    Jobs {
        thread: Option<String>,
    },
    JobOutput {
        id: String,
        after: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fault {
    pub code: String,
    pub message: String,
    pub outcome_unknown: bool,
}

impl Fault {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            outcome_unknown: false,
        }
    }
    pub fn unknown(message: &str) -> Self {
        Self {
            code: "OPERATION_OUTCOME_UNKNOWN".into(),
            message: message.into(),
            outcome_unknown: true,
        }
    }
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Fault {}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Frame {
    Result(Value),
    Error(Fault),
    Event {
        cursor: i64,
        message: Value,
    },
    Heartbeat,
    Execute {
        operation: String,
        message: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        approval: Option<Box<CommandApproval>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        permissions: Option<Box<SessionPermissions>>,
    },
    Ack {
        cursor: i64,
    },
    Detach,
}
