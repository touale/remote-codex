use remote_codex_protocol::transfer::{Kind, Stamp};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Upload,
    Download,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Paused,
    Preparing,
    Running,
    Reconnecting,
    Conflict,
    Completed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    Replace,
    KeepBoth,
    Skip,
    Merge,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conflict {
    pub path: String,
    pub source: Kind,
    pub destination: Kind,
    pub(crate) expected: Stamp,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transfer {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub server: String,
    pub workspace: String,
    pub direction: Direction,
    pub destination: String,
    pub status: Status,
    pub bytes: u64,
    pub total: u64,
    pub files: usize,
    pub completed: usize,
    pub skipped: usize,
    pub message: Option<String>,
    pub error_code: Option<String>,
    pub conflict: Option<Conflict>,
    pub active: bool,
    pub owned: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Grant {
    pub root: PathBuf,
    pub device: u64,
    pub inode: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Source {
    pub root: usize,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Task {
    pub view: Transfer,
    pub server_id: String,
    pub(crate) grants: Vec<Grant>,
    pub(crate) sources: Vec<Source>,
    pub remote_identity: Option<String>,
    #[serde(default)]
    pub remote_root: Option<remote_codex_protocol::transfer::RootIdentity>,
    pub scanned: bool,
    pub cancel: bool,
    pub current: Option<i64>,
    pub policy: Option<Choice>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Item {
    pub index: i64,
    pub parent: Option<i64>,
    pub root: usize,
    pub source: String,
    pub target: String,
    pub stamp: Stamp,
    pub token: String,
    pub expected: Option<Stamp>,
    pub prepared: bool,
    pub bytes: u64,
    pub done: bool,
    pub skipped: bool,
    pub choice: Option<Choice>,
    pub restart: bool,
    pub restage: bool,
}
pub(crate) fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.into()
    } else {
        format!("{}/{name}", parent.trim_end_matches('/'))
    }
}
