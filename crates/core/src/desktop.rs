use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
    pub description: String,
    pub efforts: Vec<String>,
    pub default_effort: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountStatus {
    pub logged_in: bool,
    pub email: Option<String>,
    pub plan: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfirmedSettings {
    pub mode: crate::goals::CollaborationMode,
    pub model: String,
    pub effort: Option<String>,
    pub full_access: bool,
    pub reviewer: String,
}
/// Read-only preview; an opened native thread remains authoritative.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionDefaults {
    pub settings: ConfirmedSettings,
    pub models: Vec<ModelOption>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub output: String,
    pub status: String,
    pub changes: Vec<FileChange>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub diff: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputField {
    pub id: String,
    pub label: String,
    pub description: String,
    pub kind: String,
    pub required: bool,
    pub choices: Vec<String>,
    pub secret: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Interaction {
    Questions {
        fields: Vec<InputField>,
    },
    McpForm {
        server: String,
        message: String,
        fields: Vec<InputField>,
    },
    McpUrl {
        server: String,
        message: String,
        url: String,
    },
    Unsupported {
        message: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InteractionAnswer {
    pub action: InteractionDecision,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionDecision {
    Accept,
    Decline,
    Cancel,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpStatus {
    pub name: String,
    pub status: String,
    pub authentication: String,
    pub tools: usize,
}

/// Volatile controls restored after an event receiver falls behind.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SessionSnapshot {
    pub status: crate::status::SessionStatus,
    pub current_turn: Option<crate::status::TurnState>,
    pub goal: Option<crate::goals::Goal>,
    pub plan: Option<crate::goals::Plan>,
    pub settings: ConfirmedSettings,
    pub environment: crate::session::EnvironmentState,
    pub turn: Option<String>,
    pub pending: Vec<crate::session::SessionEvent>,
    pub closed: bool,
}
