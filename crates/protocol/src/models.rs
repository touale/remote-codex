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
    #[serde(default)]
    pub service_instance: String,
    #[serde(default)]
    pub boot_id: String,
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

pub const EXECUTION_RECOVERY_CAPABILITY: &str = "execution-recovery-v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionStatus {
    pub channel: String,
    pub service_instance: String,
    pub boot_id: String,
    pub state: String,
    pub reason: Option<String>,
    pub replay_floor: i64,
    pub unknown_operations: i64,
    #[serde(default)]
    pub evidence_available: bool,
}
