use std::path::Path;

use super::{RuntimeInfo, download, inspect_host, release};
use crate::{
    ClientError, Result,
    connection::SshEndpoint,
    progress::{PrepareEvent, PrepareStage, TransferKind, TransferProgress},
    ssh::{SshTransport, quote},
};

pub(crate) async fn prepare(
    ssh: &SshTransport,
    endpoint: &SshEndpoint,
    cache: &Path,
    version: &str,
    known: Option<&RuntimeInfo>,
    progress: impl Fn(PrepareEvent),
) -> Result<RuntimeInfo> {
    progress(PrepareEvent::Stage(PrepareStage::InspectHost));
    let host = inspect_host(ssh, endpoint).await?;
    progress(PrepareEvent::Stage(PrepareStage::InspectRuntime));
    let package = release::resolve(cache, version, known).await?;
    let operation = uuid::Uuid::new_v4().to_string();
    let archive = format!("{}/.incoming-{operation}.tar.gz", host.data_dir);
    let bindings = format!(
        "RC_ROOT={}\nRC_VERSION={}\nRC_SHA256={}\nRC_ARCHIVE={}\nRC_OPERATION={}\n",
        quote(&host.data_dir)?,
        quote(version)?,
        quote(&package.sha256)?,
        quote(&archive)?,
        quote(&operation)?
    );
    let inspected = ssh
        .script(
            endpoint,
            &format!("{bindings}{}", include_str!("inspect.sh")),
        )
        .await?;
    if inspected.lines().last() != Some("REMOTE_CODEX_RUNTIME_READY_V1") {
        if inspected.lines().last() != Some("REMOTE_CODEX_RUNTIME_MISSING_V1") {
            return Err(ClientError::RemoteResponse);
        }
        let package =
            download::verified_package(cache, &package.url, &package.sha256, &progress).await?;
        progress(PrepareEvent::Stage(PrepareStage::Upload));
        ssh.upload(endpoint, &package, &archive, |transferred_bytes, total| {
            progress(PrepareEvent::Transfer(TransferProgress {
                kind: TransferKind::Upload,
                transferred_bytes,
                total_bytes: Some(total),
            }));
        })
        .await?;
        progress(PrepareEvent::Stage(PrepareStage::VerifyInstall));
        let installed = ssh
            .script(
                endpoint,
                &format!("{bindings}{}", include_str!("install.sh")),
            )
            .await?;
        if !matches!(
            installed.lines().last(),
            Some("REMOTE_CODEX_RUNTIME_READY_V1" | "REMOTE_CODEX_RUNTIME_REUSED_V1")
        ) {
            return Err(ClientError::RemoteResponse);
        }
    }
    progress(PrepareEvent::Stage(PrepareStage::Prepared));
    Ok(RuntimeInfo {
        version: version.to_owned(),
        platform: host.platform,
        archive_sha256: package.sha256,
    })
}
