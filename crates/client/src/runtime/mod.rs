mod download;
mod install;
pub mod tui;

use crate::{ClientError, Result, connection::SshEndpoint, ssh::SshTransport};
use serde::{Deserialize, Serialize};

pub use install::prepare;

pub const CANDIDATE_VERSION: &str = "0.153.4";
pub const LINUX_X86_64_SHA256: &str =
    "a822187e1a2420c61c5926721bfbd878701ed95547c9bb0d4de4498a16ba1821";
const ASSET_URL: &str = "https://github.com/openai/codex/releases/download/rust-v0.153.4/codex-package-x86_64-unknown-linux-musl.tar.gz";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeInfo {
    pub version: String,
    pub platform: String,
    pub executable: String,
    pub archive_sha256: String,
    pub reused: bool,
}

pub struct RemoteHost {
    pub platform: String,
    pub data_dir: String,
}

pub async fn inspect_host(ssh: &SshTransport, endpoint: &SshEndpoint) -> Result<RemoteHost> {
    let output = ssh.script(endpoint, include_str!("probe.sh")).await?;
    let (_, fields) = output
        .split_once("REMOTE_CODEX_HOST_V1\n")
        .ok_or(ClientError::RemoteResponse)?;
    let fields: Vec<_> = fields.trim_end_matches('\n').split('\n').collect();
    if fields.len() != 3 || fields[0] != "Linux" || fields[1] != "x86_64" {
        return Err(ClientError::Unsupported(
            "bootstrap currently validated only for Linux x86_64",
        ));
    }
    if !fields[2].starts_with('/') || fields[2].chars().any(char::is_control) {
        return Err(ClientError::RemoteResponse);
    }
    Ok(RemoteHost {
        platform: "linux-x86_64".to_owned(),
        data_dir: format!("{}/.local/share/remote-codex", fields[2]),
    })
}
