use super::{ServerBundle, deploy};
use crate::{
    ClientError, Result,
    progress::{PrepareEvent, PrepareStage},
    remote::Remote,
    runtime,
    ssh::SshTransport,
    store::{ConnectionRecord, LocalStore},
};
use remote_codex_adapter::catalog;
use std::path::Path;

/// An explicit add verifies reusable installations; normal connections retain the
/// saved-version fast path. Both use the same installation and activation steps.
pub(super) async fn environment(
    store: &LocalStore,
    directory: &Path,
    mut record: ConnectionRecord,
    ssh: SshTransport,
    bundle: ServerBundle<'_>,
    verify: bool,
    progress: impl Fn(PrepareEvent),
) -> Result<Remote> {
    if bundle.bytes.is_empty() {
        return Err(ClientError::Argument(
            "service bundle missing; use a packaged remote-codex distribution",
        ));
    }
    if verify
        || record.runtime.as_ref().is_none_or(|runtime| {
            runtime.version != catalog::VERSION || runtime.archive_sha256 != catalog::REMOTE_SHA256
        })
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
    let access = store.server_access(&record.id).await?;
    if verify
        || access.service_root.is_none()
        || !access
            .service_executable
            .as_deref()
            .is_some_and(|path| path.ends_with(&format!("/{}/remote-codex-server", bundle.sha256)))
    {
        progress(PrepareEvent::Stage(PrepareStage::InstallService));
        let (program, root) =
            deploy::install(&ssh, &record.endpoint, &record, bundle, &progress).await?;
        store
            .set_service_installation(&record.id, &program, &root)
            .await?;
    }
    let access = store.server_access(&record.id).await?;
    progress(PrepareEvent::Stage(PrepareStage::StartService));
    Remote::from_transport_with_progress(store, record, access, ssh, &progress).await
}
