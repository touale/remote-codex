//! Bounded binary file transfer; independent of the UTF-8 editor protocol.
pub mod wire;
use serde::{Deserialize, Serialize};
pub const CAPABILITY: &str = "file-transfer-v1";
pub const CHUNK: usize = 256 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootIdentity {
    pub device: u64,
    pub inode: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    File,
    Directory,
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    pub kind: Kind,
    pub length: u64,
    pub modified_ns: u64,
    pub device: u64,
    pub inode: u64,
    pub mode: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub stamp: Stamp,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    pub entries: Vec<Entry>,
    pub after: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Command {
    Stat {
        path: String,
    },
    List {
        path: String,
        after: Option<String>,
    },
    Read {
        path: String,
        stamp: Stamp,
        offset: u64,
    },
    Hash {
        path: String,
        stamp: Stamp,
    },
    Prepare {
        path: String,
        token: String,
        source: Stamp,
        expected: Option<Stamp>,
    },
    Write {
        path: String,
        token: String,
        offset: u64,
        digest: String,
    },
    Commit {
        path: String,
        token: String,
        digest: String,
    },
    Cancel {
        path: String,
        token: String,
    },
    Directory {
        path: String,
    },
    Metadata {
        path: String,
        stamp: Stamp,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    Stat { stamp: Option<Stamp> },
    List(Page),
    Ready { offset: u64, complete: bool },
    Data { digest: String },
    Hash { digest: String },
    Done,
}
