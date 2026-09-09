use super::{ConnectOptions, base_command};
use crate::{ClientError, Result, connection::SshEndpoint};
use serde::Serialize;
use std::{process::Stdio, time::Duration};
use tokio::process::Command;

/// Bind credentials to the effective SSH identity as well as the saved alias.
#[derive(Serialize)]
pub(crate) struct Target {
    hostname: String,
    user: String,
    port: u16,
}

impl Target {
    pub(crate) async fn resolve(options: &ConnectOptions, endpoint: &SshEndpoint) -> Result<Self> {
        let output = tokio::time::timeout(
            Duration::from_secs(15),
            base_command(options, endpoint)
                .args(["-G", "--"])
                .arg(endpoint.host())
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| ClientError::Timeout)??;
        if !output.status.success() {
            return Err(ClientError::Ssh(output.status.code().unwrap_or(255)));
        }
        let config = String::from_utf8(output.stdout).map_err(|_| ClientError::RemoteResponse)?;
        let field = |name: &str| -> Result<&str> {
            config
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
                .ok_or(ClientError::RemoteResponse)
        };
        let port = field("port ")?
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or(ClientError::RemoteResponse)?;
        Ok(Self {
            hostname: field("hostname ")?.into(),
            user: field("user ")?.into(),
            port,
        })
    }

    pub(crate) fn binding(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub(super) fn password_prompt(&self) -> String {
        format!("{}@{}'s password: ", self.user, self.hostname)
    }

    pub(super) fn pin(&self, command: &mut Command) {
        command
            .arg("-o")
            .arg(format!("Hostname={}", self.hostname))
            .arg("-l")
            .arg(&self.user)
            .arg("-p")
            .arg(self.port.to_string());
    }
}
