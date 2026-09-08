use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Environment values can be secret; never derive Debug for this type.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteConfig {
    pub codex: String,
    pub values: BTreeMap<String, String>,
    pub revision: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    pub identity: String,
    pub home: String,
    pub user: String,
    pub version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub build_id: String,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub process_id: String,
    pub kind: String,
    pub channel: String,
    pub thread: Option<String>,
    pub cwd: String,
    pub state: String,
    pub exit_code: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub output_truncated: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub executable: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCommand {
    pub argv: Vec<String>,
    pub cwd: String,
}
