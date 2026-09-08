mod deploy;
pub mod keys;
pub(crate) mod start;
pub mod status;

use crate::{
    ClientError, Result,
    config::{ConfigInput, ConfigLayer},
    connection::SshEndpoint,
    progress::{PrepareEvent, PrepareStage},
    remote::Remote,
    runtime,
    ssh::{ConnectOptions, SshTransport},
    store::LocalStore,
};
use std::path::{Path, PathBuf};

pub struct ServerBundle<'a> {
    pub bytes: &'a [u8],
    pub sha256: &'a str,
}

pub struct AddServer<'a> {
    pub name: &'a str,
    pub address: &'a str,
    pub port: Option<u16>,
    pub settings: &'a [(String, String)],
    pub identity: Option<PathBuf>,
    pub ssh_config: Option<PathBuf>,
    pub install_key: bool,
    pub interactive: bool,
}

/// Upgrade an existing saved endpoint to the service lifecycle on first use.
pub async fn ensure(
    store: &LocalStore,
    directory: &Path,
    mut record: crate::store::ConnectionRecord,
    ssh_config: Option<PathBuf>,
    interactive: bool,
    bundle: ServerBundle<'_>,
    progress: impl Fn(PrepareEvent),
) -> Result<Remote> {
    let mut access = store.server_access(&record.id).await?;
    if access.service_executable.is_some() && record.runtime.is_some() {
        progress(PrepareEvent::Stage(PrepareStage::ConnectSsh));
        // Development fixtures own their service binary. Distribution installs
        // are immutable by digest and upgraded before opening an execution.
        let managed = record
            .runtime
            .as_ref()
            .is_some_and(|r| r.executable.contains("/runtimes/codex/"));
        if !managed || bundle.bytes.is_empty() {
            return Remote::connect(store, record, ssh_config, !interactive).await;
        }
        let ssh = SshTransport::connect_with(
            ConnectOptions {
                config: ssh_config.or_else(|| access.ssh_config.clone()),
                identity_file: access.identity_file.clone(),
                batch: !interactive,
            },
            &record.endpoint,
        )
        .await?;
        if record
            .runtime
            .as_ref()
            .is_some_and(|r| r.version != runtime::CANDIDATE_VERSION)
        {
            let prepared = runtime::prepare(
                &ssh,
                &record.endpoint,
                &directory.join("cache/runtime"),
                &progress,
            )
            .await?;
            store
                .activate_runtime(&record.id, record.saved_revision, &prepared)
                .await?;
            record = store.connection_by_id(&record.id).await?;
        }
        if !access
            .service_executable
            .as_deref()
            .is_some_and(|path| path.ends_with(&format!("/{}/remote-codex-server", bundle.sha256)))
        {
            progress(PrepareEvent::Stage(PrepareStage::InstallService));
            let (program, root) =
                deploy::install(&ssh, &record.endpoint, &record, bundle, &progress).await?;
            access.service_executable = Some(program);
            access.service_root = Some(root);
            store.save_access(&record.id, &access).await?;
        }
        progress(PrepareEvent::Stage(PrepareStage::StartService));
        return Remote::from_transport_with_progress(store, record, access, ssh, &progress).await;
    }
    let address = record.endpoint.destination();
    add(
        store,
        directory,
        AddServer {
            name: &record.name,
            address: &address,
            port: record.endpoint.port(),
            settings: &[],
            identity: None,
            ssh_config,
            install_key: false,
            interactive,
        },
        bundle,
        progress,
    )
    .await
}

pub async fn add(
    store: &LocalStore,
    directory: &Path,
    request: AddServer<'_>,
    bundle: ServerBundle<'_>,
    progress: impl Fn(PrepareEvent),
) -> Result<Remote> {
    let endpoint = SshEndpoint::parse(request.address, request.port)?;
    let mut validated = ConfigLayer::new();
    for (key, value) in request.settings {
        validated.set(key, ConfigInput::Plain(value))?;
    }
    let mut record = store.save_connection(&endpoint, Some(request.name)).await?;
    let revision = store.config_snapshot(&record.id).await?.revision;
    let revision = store
        .set_many_config(&record.id, request.settings, revision)
        .await?;
    let mut access = store.server_access(&record.id).await?;
    if let Some(identity) = request.identity {
        access.identity_file = Some(identity.canonicalize()?);
    }
    if let Some(config) = request.ssh_config {
        access.ssh_config = Some(config.canonicalize()?);
    }
    store.save_access(&record.id, &access).await?;
    progress(PrepareEvent::Stage(PrepareStage::ConnectSsh));
    let ssh = SshTransport::connect_with(
        ConnectOptions {
            config: access.ssh_config.clone(),
            identity_file: access.identity_file.clone(),
            batch: !request.interactive,
        },
        &endpoint,
    )
    .await?;
    if request.install_key {
        if !request.interactive {
            return Err(ClientError::Argument(
                "install-key requires an interactive terminal; scripts should supply an existing identity",
            ));
        }
        let installed = keys::install(&ssh, &endpoint, directory, &record.id, &mut access).await;
        store.save_access(&record.id, &access).await?;
        installed?;
    }
    let runtime =
        runtime::prepare(&ssh, &endpoint, &directory.join("cache/runtime"), &progress).await?;
    store
        .record_runtime(&record.id, revision.saved, &runtime)
        .await?;
    record.runtime = Some(runtime);
    progress(PrepareEvent::Stage(PrepareStage::InstallService));
    let (program, root) = deploy::install(&ssh, &endpoint, &record, bundle, &progress).await?;
    access.service_executable = Some(program);
    access.service_root = Some(root);
    store.save_access(&record.id, &access).await?;
    progress(PrepareEvent::Stage(PrepareStage::StartService));
    let remote =
        Remote::from_transport_with_progress(store, record, access, ssh, &progress).await?;
    progress(PrepareEvent::Stage(PrepareStage::Synchronize));
    remote.synchronize(store).await?;
    Ok(remote)
}
