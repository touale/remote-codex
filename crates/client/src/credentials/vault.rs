use super::locking::WritePermit;
use crate::{ClientError, Result, config::SecretRef};
use std::{future::Future, sync::Arc};
use zeroize::Zeroizing;

pub(crate) trait CredentialVault: Send + Sync {
    fn put(
        &self,
        reference: &SecretRef,
        value: &str,
        permit: Arc<WritePermit>,
    ) -> impl Future<Output = Result<()>> + Send;
    fn read(&self, reference: &SecretRef)
    -> impl Future<Output = Result<Zeroizing<String>>> + Send;
    fn delete(&self, reference: &SecretRef) -> impl Future<Output = Result<()>> + Send;
}

pub(crate) struct NativeVault {
    service: String,
}

impl NativeVault {
    pub(crate) fn new(installation_id: &str) -> Self {
        Self {
            service: format!("remote-codex.{installation_id}"),
        }
    }
}

impl CredentialVault for NativeVault {
    async fn put(
        &self,
        reference: &SecretRef,
        value: &str,
        permit: Arc<WritePermit>,
    ) -> Result<()> {
        let service = self.service.clone();
        let account = reference.id().to_owned();
        let value = Zeroizing::new(value.to_owned());
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            native_put(&service, &account, value.as_bytes())
        })
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
