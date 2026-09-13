use super::ServerBundle;
use crate::{
    ClientError, Result,
    connection::SshEndpoint,
    progress::{PrepareEvent, TransferKind, TransferProgress},
    ssh::{SshTransport, quote},
};
use sha2::{Digest, Sha256};
use std::io::Write;

pub(super) async fn install(
    ssh: &SshTransport,
    endpoint: &SshEndpoint,
    root: &str,
    bundle: ServerBundle<'_>,
    progress: impl Fn(PrepareEvent),
) -> Result<(String, String)> {
    if bundle.bytes.is_empty() {
        return Err(ClientError::Argument(
            "this build has no bundled Linux service; build the distribution with tools/package.py",
        ));
    }
    if format!("{:x}", Sha256::digest(bundle.bytes)) != bundle.sha256 {
        return Err(ClientError::Integrity);
    }
    let program = format!(
        "{root}/runtimes/service/{}/remote-codex-server",
        bundle.sha256
    );
    let found = ssh
        .script(
            endpoint,
            &format!(
                "set -eu\numask 077; test ! -L {}; mkdir -p {}; test \"$(stat -c %a {})\" = 700; if test -f {}; then sha256sum {}; fi\n",
                quote(root)?,
                quote(root)?,
                quote(root)?,
                quote(&program)?,
                quote(&program)?
            ),
        )
        .await?;
    if found.split_whitespace().next() == Some(bundle.sha256) {
        return Ok((program, format!("{root}/execution-v3")));
    }
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(bundle.bytes)?;
    file.as_file().sync_all()?;
    let incoming = format!("{root}/.service-{}", uuid::Uuid::new_v4());
    ssh.upload(
        endpoint,
        file.path(),
        &incoming,
        |transferred_bytes, total| {
            progress(PrepareEvent::Transfer(TransferProgress {
                kind: TransferKind::Upload,
                transferred_bytes,
                total_bytes: Some(total),
            }))
        },
    )
    .await?;
    let bindings = format!(
        "RC_ROOT={}\nRC_INCOMING={}\nRC_HASH={}\n",
        quote(root)?,
        quote(&incoming)?,
        quote(bundle.sha256)?
    );
    let output = ssh
        .script(
            endpoint,
            &format!("{bindings}{}", include_str!("deploy.sh")),
        )
        .await?;
    if output.lines().last() != Some("REMOTE_CODEX_SERVICE_INSTALLED_V1") {
        return Err(ClientError::RemoteResponse);
    }
    Ok((program, format!("{root}/execution-v3")))
}
