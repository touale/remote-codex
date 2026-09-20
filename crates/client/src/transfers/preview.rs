use super::peer::Peer;
use crate::progress::{PrepareEvent, TransferKind, TransferProgress};
use crate::{ClientError, Result, remote::Remote};
use remote_codex_protocol::transfer::{Command, Kind, Reply};

const MAX_PREVIEW_BYTES: u64 = 50 * 1024 * 1024;

/// A bounded, read-only use of the existing transfer wire, without a saved job.
pub(crate) async fn read(remote: &Remote, root: &str, path: &str) -> Result<Vec<u8>> {
    let mut peer = Peer::open(remote, root, None).await?;
    let (reply, _) = peer.call(Command::Stat { path: path.into() }, &[]).await?;
    let Reply::Stat { stamp: Some(stamp) } = reply else {
        return Err(ClientError::Argument("This file no longer exists."));
    };
    if stamp.kind != Kind::File {
        return Err(ClientError::Argument(
            "Preview supports regular files only.",
        ));
    }
    if stamp.length > MAX_PREVIEW_BYTES {
        return Err(ClientError::RemoteFault(
            "PREVIEW_TOO_LARGE".into(),
            "Preview supports files up to 50 MiB. Download this file to open it locally.".into(),
            false,
        ));
    }
    let progress = crate::progress::current();
    let report = |bytes| {
        if let Some(handler) = &progress {
            handler(PrepareEvent::Transfer(TransferProgress {
                kind: TransferKind::Download,
                transferred_bytes: bytes,
                total_bytes: Some(stamp.length),
            }));
        }
    };
    let mut bytes = Vec::with_capacity(stamp.length as usize);
    report(0);
    while (bytes.len() as u64) < stamp.length {
        let (reply, chunk) = peer
            .call(
                Command::Read {
                    path: path.into(),
                    stamp: stamp.clone(),
                    offset: bytes.len() as u64,
                },
                &[],
            )
            .await?;
        let Reply::Data { digest } = reply else {
            return Err(ClientError::RemoteResponse);
        };
        if chunk.is_empty()
            || bytes.len() as u64 + chunk.len() as u64 > stamp.length
            || remote_codex_transfer::digest(&chunk) != digest
        {
            return Err(ClientError::Integrity);
        }
        bytes.extend(chunk);
        report(bytes.len() as u64);
    }
    let (reply, _) = peer
        .call(
            Command::Hash {
                path: path.into(),
                stamp,
            },
            &[],
        )
        .await?;
    let Reply::Hash { digest } = reply else {
        return Err(ClientError::RemoteResponse);
    };
    if remote_codex_transfer::digest(&bytes) != digest {
        return Err(ClientError::Integrity);
    }
    Ok(bytes)
}
