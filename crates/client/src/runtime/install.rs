use std::path::Path;

use super::{RuntimeInfo, download, inspect_host};
use crate::{
    ClientError, Result,
    connection::SshEndpoint,
    progress::{PrepareEvent, PrepareStage, TransferKind, TransferProgress},
    ssh::{SshTransport, quote},
};
use remote_codex_adapter::catalog;

pub(crate) async fn prepare(
    ssh: &SshTransport,
    endpoint: &SshEndpoint,
    cache: &Path,
    progress: impl Fn(PrepareEvent),
) -> Result<RuntimeInfo> {
    progress(PrepareEvent::Stage(PrepareStage::InspectHost));
    let host = inspect_host(ssh, endpoint).await?;
    let operation = uuid::Uuid::new_v4().to_string();
    let archive = format!("{}/.incoming-{operation}.tar.gz", host.data_dir);
    let bindings = format!(
        "RC_ROOT={}\nRC_VERSION={}\nRC_SHA256={}\nRC_ARCHIVE={}\nRC_OPERATION={}\n",
        quote(&host.data_dir)?,
        quote(catalog::VERSION)?,
        quote(catalog::REMOTE_SHA256)?,
        quote(&archive)?,
        quote(&operation)?
    );
    progress(PrepareEvent::Stage(PrepareStage::InspectRuntime));
    let inspected = ssh
        .script(
            endpoint,
            &format!("{bindings}{}", include_str!("inspect.sh")),
        )
        .await?;
    let mut reused = inspected.lines().last() == Some("REMOTE_CODEX_RUNTIME_READY_V1");
    if !reused {
        if inspected.lines().last() != Some("REMOTE_CODEX_RUNTIME_MISSING_V1") {
            return Err(ClientError::RemoteResponse);
        }
        let package = download::package(cache, &progress).await?;
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
        reused = match installed.lines().last() {
            Some("REMOTE_CODEX_RUNTIME_READY_V1") => false,
            Some("REMOTE_CODEX_RUNTIME_REUSED_V1") => true,
            _ => return Err(ClientError::RemoteResponse),
        };
    }
    progress(PrepareEvent::Stage(PrepareStage::Prepared));
    Ok(RuntimeInfo {
        version: catalog::VERSION.to_owned(),
        platform: host.platform,
        executable: format!(
            "{}/runtimes/codex/{}-linux-x86_64/bin/codex",
            host.data_dir,
            catalog::VERSION
        ),
        archive_sha256: catalog::REMOTE_SHA256.to_owned(),
        reused,
    })
}
