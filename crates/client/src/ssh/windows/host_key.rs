use super::{authentication::prompt, config::Config};
use crate::{
    ClientError, Result,
    ssh::interaction::{AuthenticationHandler, AuthenticationKind},
};
use russh::{
    client,
    keys::{PublicKeyOrCertificate, ssh_key::HashAlg},
};
use std::path::PathBuf;

pub(super) struct Verifier {
    host: String,
    port: u16,
    path: PathBuf,
    handler: Option<AuthenticationHandler>,
}
impl Verifier {
    pub(super) fn new(config: &Config, handler: Option<AuthenticationHandler>) -> Self {
        Self {
            host: config.target.hostname.clone(),
            port: config.target.port,
            path: config.known_hosts.clone(),
            handler,
        }
    }
}
impl From<russh::Error> for ClientError {
    fn from(error: russh::Error) -> Self {
        super::failure(error)
    }
}
impl client::Handler for Verifier {
    type Error = ClientError;
    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            return Err(ClientError::Unsupported("SSH host certificates on Windows"));
        };
        let invalid = || {
            remote_codex_protocol::Fault::new(
                "SSH_IDENTITY_REQUIRED",
                "The SSH host key changed or is revoked. Review the host's identity before reconnecting.",
            )
        };
        // The library's known_hosts parser ignores marker lines. Never accept a revoked key.
        if let Ok(text) = std::fs::read_to_string(&self.path) {
            for line in text
                .lines()
                .filter(|line| line.trim_start().starts_with('@'))
            {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.first() == Some(&"@revoked")
                    && fields.len() >= 4
                    && russh::keys::ssh_key::PublicKey::from_openssh(&format!(
                        "{} {}",
                        fields[2], fields[3]
                    ))
                    .is_ok_and(|revoked| revoked == *key)
                {
                    return Err(invalid().into());
                }
                if fields.first() == Some(&"@cert-authority") {
                    return Err(ClientError::Unsupported(
                        "SSH certificate authority known_hosts entries on Windows",
                    ));
                }
            }
        }
        match russh::keys::check_known_hosts_path(&self.host, self.port, key, &self.path) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(_) => return Err(invalid().into()),
        }
        let answer = prompt(
            self.handler.as_ref(),
            &self.host,
            AuthenticationKind::HostKey,
            format!(
                "Trust SSH host {}:{}?\n{} {}\nType yes to continue.",
                self.host,
                self.port,
                key.algorithm(),
                key.fingerprint(HashAlg::Sha256)
            ),
        )
        .await?;
        if answer.trim() != "yes" {
            return Ok(false);
        }
        let parent = self.path.parent().ok_or(ClientError::RemoteResponse)?;
        if !parent.exists() {
            remote_codex_platform::fs::private_dir(parent)?;
        }
        // Serialize known_hosts writers from multiple app windows and CLI processes.
        let lock = remote_codex_platform::fs::open_private(
            &parent.join("remote-codex-hosts.lock"),
            remote_codex_platform::fs::Creation::IfMissing,
        )?;
        let _lock = remote_codex_platform::fs::FileLock::lock(
            lock,
            remote_codex_platform::fs::LockMode::Exclusive,
        )?;
        match russh::keys::check_known_hosts_path(&self.host, self.port, key, &self.path) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(_) => return Err(invalid().into()),
        }
        russh::keys::known_hosts::learn_known_hosts_path(&self.host, self.port, key, &self.path)
            .map_err(|_| invalid())?;
        Ok(true)
    }
}
