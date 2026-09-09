use crate::{
    Result,
    config::SecretRef,
    credentials::{CredentialVault, NativeVault},
    store::LocalStore,
};

/// Resolve on every connection attempt, including recovery, so removal and rotation take effect.
#[derive(Clone)]
pub(crate) struct Credentials {
    store: LocalStore,
    server: String,
    pending: Option<SecretRef>,
}

impl Credentials {
    pub(crate) fn saved(store: &LocalStore, server: &str) -> Self {
        Self {
            store: store.clone(),
            server: server.into(),
            pending: None,
        }
    }

    pub(crate) fn pending(store: &LocalStore, server: &str, reference: SecretRef) -> Self {
        Self {
            store: store.clone(),
            server: server.into(),
            pending: Some(reference),
        }
    }

    pub(super) async fn resolve(
        &self,
        options: &super::ConnectOptions,
        endpoint: &crate::connection::SshEndpoint,
    ) -> Result<Option<(super::target::Target, zeroize::Zeroizing<String>)>> {
        let reference = match &self.pending {
            Some(reference) => Some(reference.clone()),
            None => self.store.ssh_credential(&self.server).await?,
        };
        let Some(reference) = reference else {
            return Ok(None);
        };
        let target = super::target::Target::resolve(options, endpoint).await?;
        if self
            .store
            .ssh_credential_target(&self.server, &reference)
            .await?
            != target.binding()?
        {
            return Err(remote_codex_protocol::Fault::new("SSH_CREDENTIAL_TARGET_CHANGED",
                "SSH configuration now resolves to a different host, port or user; review it before saving a password with server auth -n NAME").into());
        }
        let installation = self.store.installation_id().await?;
        // Fail as a vault error before starting SSH if the keychain is locked or missing.
        let vault = NativeVault::new(&installation);
        let value = vault.read(&reference).await?;
        crate::credentials::validate_password(&value)?;
        Ok(Some((target, value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn changed_ssh_alias_is_rejected_before_reading_the_vault() -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = LocalStore::open(&root.path().join("state")).await?;
        let endpoint = crate::connection::SshEndpoint::parse("development", None)?;
        let record = store.save_connection(&endpoint, Some("dev")).await?;
        let config = root.path().join("ssh_config");
        std::fs::write(
            &config,
            "Host development\n HostName first.invalid\n User developer\n Port 22\n",
        )?;
        let options = crate::ssh::ConnectOptions {
            config: Some(config.clone()),
            ..Default::default()
        };
        let reference = SecretRef::new("not-present-in-vault")?;
        let target = crate::ssh::target::Target::resolve(&options, &endpoint)
            .await?
            .binding()?;
        store
            .reserve_ssh_credential(&record.id, &reference, &target)
            .await?;
        store
            .activate_ssh_credential(&record.id, None, &reference)
            .await?;
        let credentials = Credentials::saved(&store, &record.id);
        for (host, user, port) in [
            ("second.invalid", "developer", 22),
            ("first.invalid", "other", 22),
            ("first.invalid", "developer", 2222),
        ] {
            std::fs::write(
                &config,
                format!("Host development\n HostName {host}\n User {user}\n Port {port}\n"),
            )?;
            let result = credentials.resolve(&options, &endpoint).await;
            assert!(
                matches!(result, Err(error) if error.code() == "SSH_CREDENTIAL_TARGET_CHANGED")
            );
        }
        store.close().await;
        Ok(())
    }
}
