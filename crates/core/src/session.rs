use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived: bool,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBinding {
    pub server_id: String,
    pub remote_identity: String,
    pub environment_id: String,
    pub codex_home: String,
    pub codex_version: String,
    pub execution_mode: String,
    pub revision: i64,
    pub session: Session,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionPreset {
    Workspace,
    FullAccess,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalsReviewer {
    User,
    AutoReview,
    GuardianSubagent,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionSettings {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permissions: Option<PermissionPreset>,
    pub reviewer: Option<ApprovalsReviewer>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    AcceptOnce,
    Cancel,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    EnvironmentChanged {
        state: EnvironmentState,
    },
    Message {
        item_id: String,
        text: String,
        complete: bool,
    },
    ToolOutput {
        item_id: String,
        text: String,
    },
    ApprovalRequested {
        request_id: String,
        description: String,
    },
    InteractionRequired {
        request_id: String,
        kind: String,
    },
    TurnStarted {
        id: String,
    },
    TurnCompleted {
        id: String,
        outcome: TurnOutcome,
    },
    PermissionsUpdated {
        full_access: bool,
    },
    Closed {
        reason: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EnvironmentState {
    Ready,
    Reconnecting { attempt: u32, retry_in_ms: u64 },
    Recovering { reason: String },
    ActionRequired { code: String, message: String },
    Closed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    Interrupted,
    Failed { message: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryPage {
    pub session: Session,
    pub turns: Vec<HistoryTurn>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryTurn {
    pub id: String,
    pub status: String,
    pub items: Vec<HistoryItem>,
}

/// A display projection; the native Codex history remains the source of truth.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CachedSession {
    pub server_id: String,
    pub server: String,
    pub remote_identity: String,
    pub checked_at: i64,
    pub session: Session,
}
