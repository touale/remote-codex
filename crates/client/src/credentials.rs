pub(crate) mod locking;
mod vault;
use crate::{
    ClientError, Result,
    config::{ConfigInput, SecretRef},
    store::{LocalStore, Revision},
};
pub(crate) use vault::{CredentialVault, NativeVault};

pub fn validate_password(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 1023 || value.contains(['\0', '\n', '\r']) {
        return Err(ClientError::Argument(
            "SSH password must contain 1–1023 bytes without NUL or line breaks",
        ));
    }
    Ok(())
}

/// Validate before writing to the vault. Record intent first so a crash between
/// the vault write and SQLite commit leaves a discoverable credential handle.
pub(crate) async fn set_secret(
    store: &LocalStore,
    vault: &impl CredentialVault,
    server: &str,
    key: &str,
    raw: &str,
    expected: Revision,
) -> Result<Revision> {
    let mut snapshot = store.config_snapshot(server).await?;
    if snapshot.revision != expected {
        return Err(ClientError::RevisionConflict);
    }
    let reference = SecretRef::new(&uuid::Uuid::new_v4().to_string())?;
    snapshot.layer.set(
        key,
        ConfigInput::Secret {
            reference: reference.clone(),
            raw,
        },
    )?;
    let permit = locking::write(&store.directory).await?;
    store.reserve_credential(&reference).await?;
    let result = async {
        vault.put(&reference, raw, permit.clone()).await?;
        store
            .set_config(
                server,
                key,
                ConfigInput::Secret {
                    reference: reference.clone(),
                    raw,
                },
                expected,
            )
            .await
    }
    .await;
    if result.is_err() {
        // Preserve the primary error; pending records also remain discoverable.
        let _ = store.retire_credential(&reference).await;
    }
    result
}

/// Cleanup never changes an operation's committed result. Failed deletions remain retryable.
pub(crate) async fn cleanup(store: &LocalStore, vault: &impl CredentialVault) -> Result<bool> {
    if !store.has_credential_cleanup().await? {
        return Ok(true);
    }
    let Some(_lock) = locking::cleanup(&store.directory)? else {
        return Ok(false);
    };
    let mut complete = true;
    for credential in store.retired_credentials().await? {
        if vault.delete(credential.reference()).await.is_err()
            || store.forget_retired_credential(&credential).await.is_err()
        {
            complete = false;
        }
    }
    Ok(complete)
}
