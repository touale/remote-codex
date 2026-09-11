use serde::{Deserialize, Serialize};

pub const WORKSPACE_FILES_CAPABILITY: &str = "workspace-files-v1";
pub const FILE_CHUNK: usize = 128 * 1024;
pub const MAX_TEXT_FILE: usize = 4 * 1024 * 1024;

/// Manual workspace operations never enter the native AI execution gateway.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FileRequest {
    List {
        path: String,
    },
    Read {
        path: String,
        offset: u64,
        revision: Option<String>,
    },
    BeginWrite {
        path: String,
        revision: Option<String>,
        length: u64,
    },
    WriteChunk {
        token: String,
        offset: u64,
        bytes: Vec<u8>,
    },
    CommitWrite {
        token: String,
    },
    CancelWrite {
        token: String,
    },
    CreateDirectory {
        path: String,
    },
    Rename {
        path: String,
        destination: String,
    },
    Remove {
        path: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub directory: bool,
    pub symlink: bool,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectoryPage {
    pub entries: Vec<DirectoryEntry>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileChunk {
    pub bytes: Vec<u8>,
    pub revision: String,
    pub length: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextFile {
    pub path: String,
    pub text: String,
    pub revision: String,
}
