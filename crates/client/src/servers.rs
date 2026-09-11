pub(crate) mod authentication;
mod deploy;
pub(crate) mod keys;
mod prepare;
pub(crate) mod start;
pub(crate) mod status;

use crate::{
    ClientError, Result,
    config::{ConfigInput, ConfigLayer},
    connection::SshEndpoint,
    progress::{PrepareEvent, PrepareStage},
    remote::Remote,
    ssh::{ConnectOptions, SshTransport},
    store::LocalStore,
};
use std::path::{Path, PathBuf};

pub(crate) struct ServerBundle<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) sha256: &'a str,
}

pub(crate) struct AddServer<'a> {
    pub(crate) name: &'a str,
    pub(crate) address: &'a str,
    pub(crate) port: Option<u16>,
    pub(crate) settings: &'a [(String, String)],
    pub(crate) identity: Option<PathBuf>,
    pub(crate) ssh_config: Option<PathBuf>,
    pub(crate) install_key: bool,
    pub(crate) interactive: bool,
    pub(crate) password: Option<zeroize::Zeroizing<String>>,
}

/// Prepare a saved environment, including retries after incomplete initialization.
pub(crate) async fn ensure(
    store: &LocalStore,
    directory: &Path,
    record: crate::store::ConnectionRecord,
    ssh_config: Option<PathBuf>,
    interactive: bool,
    bundle: ServerBundle<'_>,
    progress: impl Fn(PrepareEvent),
) -> Result<Remote> {
    let access = store.server_access(&record.id).await?;
    progress(PrepareEvent::Stage(PrepareStage::ConnectSsh));
    let ssh = SshTransport::connect_with(
        ConnectOptions {
            interaction: crate::ssh::interaction::current(),
            config: ssh_config.or(access.ssh_config),
            identity_file: access.identity_file,
            batch: !interactive,
            credentials: Some(crate::ssh::credentials::Credentials::saved(
                store, &record.id,
            )),
        },
        &record.endpoint,
    )
    .await?;
    prepare::environment(store, directory, record, ssh, bundle, false, progress).await
}

pub(crate) async fn add(
    store: &LocalStore,
    directory: &Path,
    request: AddServer<'_>,
    bundle: ServerBundle<'_>,
    progress: impl Fn(PrepareEvent),
) -> Result<Remote> {
    let endpoint = SshEndpoint::parse(request.address, request.port)?;
    if let Some(password) = &request.password {
        crate::credentials::validate_password(password)?;
    }
    let mut validated = ConfigLayer::new();
    for (key, value) in request.settings {
        validated.set(key, ConfigInput::Plain(value))?;
    }
    let record = store.save_connection(&endpoint, Some(request.name)).await?;
    let revision = store.config_snapshot(&record.id).await?.revision;
    store
        .set_many_config(&record.id, request.settings, revision)
        .await?;
    let identity = request
        .identity
        .map(|path| path.canonicalize())
        .transpose()?;
    let config = request
        .ssh_config
        .map(|path| path.canonicalize())
        .transpose()?;
    store
        .set_ssh_options(&record.id, identity.as_deref(), config.as_deref())
        .await?;
    let access = store.server_access(&record.id).await?;
    progress(PrepareEvent::Stage(PrepareStage::ConnectSsh));
    let ssh = authentication::connect(
        store,
        &record,
        ConnectOptions {
            interaction: crate::ssh::interaction::current(),
            config: access.ssh_config.clone(),
            identity_file: access.identity_file.clone(),
            batch: !request.interactive,
            credentials: Some(crate::ssh::credentials::Credentials::saved(
                store, &record.id,
            )),
        },
        request.password,
    )
    .await?;
    if request.install_key {
        if !request.interactive {
            return Err(ClientError::Argument(
                "install-key requires an interactive terminal; scripts should supply an existing identity",
            ));
        }
        keys::install(&ssh, &endpoint, directory, &record.id, store).await?;
    }
    let record = store.connection_by_id(&record.id).await?;
    prepare::environment(store, directory, record, ssh, bundle, true, progress).await
}
