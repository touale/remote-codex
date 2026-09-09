use crate::{
    ClientError, Result,
    config::SecretRef,
    credentials::{CredentialVault, NativeVault, locking},
    ssh::{ConnectOptions, SshTransport, credentials::Credentials},
    store::{ConnectionRecord, LocalStore},
};
use zeroize::Zeroizing;

/// The vault write is staged. Only a successful password-only SSH handshake
/// activates it; a rejected password cannot replace a working credential.
pub(crate) async fn connect(
    store: &LocalStore,
    record: &ConnectionRecord,
    mut options: ConnectOptions,
    password: Option<Zeroizing<String>>,
) -> Result<SshTransport> {
    let Some(password) = password else {
        return SshTransport::connect_with(options, &record.endpoint).await;
    };
    crate::credentials::validate_password(&password)?;
    let vault = NativeVault::new(&store.installation_id().await?);
    let permit = locking::write(&store.directory).await?;
    let previous = store.ssh_credential(&record.id).await?;
    let target = crate::ssh::target::Target::resolve(&options, &record.endpoint)
        .await?
        .binding()?;
    let reference = SecretRef::new(&uuid::Uuid::new_v4().to_string())?;
    store
        .reserve_ssh_credential(&record.id, &reference, &target)
        .await?;
    let result = async {
        vault.put(&reference, &password, permit.clone()).await?;
        drop(password);
        options.credentials = Some(Credentials::pending(store, &record.id, reference.clone()));
        let mut ssh = SshTransport::connect_with(options, &record.endpoint).await?;
        store
            .activate_ssh_credential(&record.id, previous.as_ref(), &reference)
            .await?;
        ssh.saved_credentials(store, &record.id);
        Ok::<_, ClientError>(ssh)
    }
    .await;
    if result.is_err() {
        let _ = store.retire_ssh_credential(&reference).await;
    }
    result
}
