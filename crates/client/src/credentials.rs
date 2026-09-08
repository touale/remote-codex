use std::future::Future;

use crate::{
    ClientError, Result,
    config::{ConfigInput, SecretRef},
    store::{LocalStore, Revision},
};
use zeroize::Zeroizing;

pub trait CredentialVault: Send + Sync {
    fn put(&self, reference: &SecretRef, value: &str) -> impl Future<Output = Result<()>> + Send;
    fn read(&self, reference: &SecretRef)
    -> impl Future<Output = Result<Zeroizing<String>>> + Send;
    fn delete(&self, reference: &SecretRef) -> impl Future<Output = Result<()>> + Send;
}

pub struct NativeVault {
    service: String,
}

impl NativeVault {
    pub fn new(installation_id: &str) -> Self {
        Self {
            service: format!("remote-codex.{installation_id}"),
        }
    }
}

impl CredentialVault for NativeVault {
    async fn put(&self, reference: &SecretRef, value: &str) -> Result<()> {
        let service = self.service.clone();
        let account = reference.id().to_owned();
        let value = Zeroizing::new(value.to_owned());
        tokio::task::spawn_blocking(move || native_put(&service, &account, value.as_bytes()))
            .await
            .map_err(|_| ClientError::Credentials)?
    }

    async fn read(&self, reference: &SecretRef) -> Result<Zeroizing<String>> {
        let service = self.service.clone();
        let account = reference.id().to_owned();
        tokio::task::spawn_blocking(move || native_read(&service, &account))
            .await
            .map_err(|_| ClientError::Credentials)?
    }

    async fn delete(&self, reference: &SecretRef) -> Result<()> {
        let service = self.service.clone();
        let account = reference.id().to_owned();
        tokio::task::spawn_blocking(move || native_delete(&service, &account))
            .await
            .map_err(|_| ClientError::Credentials)?
    }
}

/// Validate before writing to the vault. Record intent first so a crash between
/// the vault write and SQLite commit leaves a discoverable credential handle.
pub async fn set_secret(
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
    store.reserve_credential(&reference).await?;
    vault.put(&reference, raw).await?;
    let result = store
        .set_config(
            server,
            key,
            ConfigInput::Secret {
                reference: reference.clone(),
                raw,
            },
            expected,
        )
        .await;
    if result.is_err() {
        // If removal fails, the pending record remains for later reconciliation.
        vault.delete(&reference).await?;
        store.forget_pending_credential(&reference).await?;
    }
    result
}

#[cfg(target_os = "macos")]
fn native_put(service: &str, account: &str, value: &[u8]) -> Result<()> {
    security_framework::passwords::set_generic_password(service, account, value)
        .map_err(|_| ClientError::Credentials)
}
#[cfg(target_os = "macos")]
fn native_read(service: &str, account: &str) -> Result<Zeroizing<String>> {
    let bytes = Zeroizing::new(
        security_framework::passwords::get_generic_password(service, account)
            .map_err(|_| ClientError::Credentials)?,
    );
    Ok(Zeroizing::new(
        std::str::from_utf8(&bytes)
            .map_err(|_| ClientError::Credentials)?
            .to_owned(),
    ))
}
#[cfg(target_os = "macos")]
fn native_delete(service: &str, account: &str) -> Result<()> {
    match security_framework::passwords::delete_generic_password(service, account) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == -25300 => Ok(()), // errSecItemNotFound: already absent.
        Err(_) => Err(ClientError::Credentials),
    }
}
#[cfg(not(target_os = "macos"))]
fn native_put(_: &str, _: &str, _: &[u8]) -> Result<()> {
    Err(ClientError::Unsupported(
        "local credential backend on this platform",
    ))
}
#[cfg(not(target_os = "macos"))]
fn native_read(_: &str, _: &str) -> Result<Zeroizing<String>> {
    Err(ClientError::Unsupported(
        "local credential backend on this platform",
    ))
}
#[cfg(not(target_os = "macos"))]
fn native_delete(_: &str, _: &str) -> Result<()> {
    Err(ClientError::Unsupported(
        "local credential backend on this platform",
    ))
}
