use super::{ServerBundle, deploy};
use crate::{
    ClientError, Result,
    progress::{PrepareEvent, PrepareStage},
    remote::Remote,
    runtime,
    ssh::SshTransport,
    store::{ConnectionRecord, LocalStore},
};
use std::path::Path;

/// Server setup prepares the selected Codex; ordinary SSH connections only ensure the service.
pub(super) async fn environment(
    store: &LocalStore,
    directory: &Path,
    mut record: ConnectionRecord,
    ssh: SshTransport,
    bundle: ServerBundle<'_>,
    version: Option<&str>,
    progress: impl Fn(PrepareEvent),
) -> Result<Remote> {
    if bundle.bytes.is_empty() {
        return Err(ClientError::Argument(
            "service bundle missing; use a packaged remote-codex distribution",
        ));
    }
    let verify = version.is_some();
    if let Some(version) = version {
        let prepared = runtime::prepare(
            &ssh,
            &record.endpoint,
            &directory.join("cache/runtime"),
            version,
            record.runtime.as_ref(),
            &progress,
        )
        .await?;
        store.remember_runtime(&record.id, &prepared).await?;
        record.runtime = Some(prepared);
    }
    let access = store.server_access(&record.id).await?;
    if verify
        || access.service_root.is_none()
        || !access
            .service_executable
            .as_deref()
            .is_some_and(|path| path.ends_with(&format!("/{}/remote-codex-server", bundle.sha256)))
    {
        progress(PrepareEvent::Stage(PrepareStage::InstallService));
        let host = runtime::inspect_host(&ssh, &record.endpoint).await?;
        let (program, root) =
            deploy::install(&ssh, &record.endpoint, &host.data_dir, bundle, &progress).await?;
        store
            .set_service_installation(&record.id, &program, &root)
            .await?;
    }
    let access = store.server_access(&record.id).await?;
    progress(PrepareEvent::Stage(PrepareStage::StartService));
    Remote::from_transport_with_progress(store, record, access, ssh, &progress).await
}
