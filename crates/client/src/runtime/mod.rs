mod download;
mod install;

use crate::{ClientError, Result, connection::SshEndpoint, ssh::SshTransport};
use serde::{Deserialize, Serialize};

pub(crate) use install::prepare;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuntimeInfo {
    pub(crate) version: String,
    pub(crate) platform: String,
    pub(crate) executable: String,
    pub(crate) archive_sha256: String,
    pub(crate) reused: bool,
}

pub(crate) struct RemoteHost {
    pub(crate) platform: String,
    pub(crate) data_dir: String,
}

pub(crate) async fn inspect_host(ssh: &SshTransport, endpoint: &SshEndpoint) -> Result<RemoteHost> {
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
        platform: remote_codex_adapter::catalog::REMOTE_TARGET.to_owned(),
        data_dir: format!("{}/.local/share/remote-codex", fields[2]),
    })
}
