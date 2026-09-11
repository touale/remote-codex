use super::{Progress, model::*, peer::Peers};
use crate::{ClientError, Result, store::LocalStore};
use remote_codex_protocol::transfer::{Command, Reply};

pub(super) async fn file(
    store: &LocalStore,
    task: &mut Task,
    peers: &mut Peers,
    item: &mut Item,
    progress: &Progress,
) -> Result<()> {
    item.prepared = true;
    store.transfer_item(&task.view.id, item).await?;
    let (reply, _) = peers
        .call(
            false,
            item.root,
            Command::Prepare {
                path: item.target.clone(),
                token: item.token.clone(),
                source: item.stamp.clone(),
                expected: item.expected.clone(),
            },
            Vec::new(),
        )
        .await?;
    let Reply::Ready {
        mut offset,
        complete,
    } = reply
    else {
        return Err(ClientError::RemoteResponse);
    };
    if offset > item.stamp.length {
        return Err(ClientError::RemoteResponse);
    }
    task.view.bytes = task
        .view
        .bytes
        .saturating_sub(item.bytes)
        .saturating_add(offset);
    item.bytes = offset;
    item.prepared = true;
    store.transfer_item(&task.view.id, item).await?;
    if complete {
        return Ok(());
    }
    let mut last = std::time::Instant::now();
    while offset < item.stamp.length {
        let (reply, bytes) = peers
            .call(
                true,
                item.root,
                Command::Read {
                    path: item.source.clone(),
                    stamp: item.stamp.clone(),
                    offset,
                },
                Vec::new(),
            )
            .await?;
        let Reply::Data { digest } = reply else {
            return Err(ClientError::RemoteResponse);
        };
        if bytes.is_empty() || remote_codex_transfer::digest(&bytes) != digest {
            return Err(ClientError::Integrity);
        }
        let size = bytes.len() as u64;
        let (reply, _) = peers
            .call(
                false,
                item.root,
                Command::Write {
                    path: item.target.clone(),
                    token: item.token.clone(),
                    offset,
                    digest,
                },
                bytes,
            )
            .await?;
        let Reply::Ready {
            offset: confirmed,
            complete: false,
        } = reply
        else {
            return Err(ClientError::RemoteResponse);
        };
        if confirmed != offset + size || confirmed > item.stamp.length {
            return Err(ClientError::RemoteResponse);
        }
        offset = confirmed;
        item.bytes = offset;
        task.view.bytes += size;
        store.transfer_item(&task.view.id, item).await?;
        if last.elapsed().as_millis() >= 100 {
            progress(task.view.clone());
            last = std::time::Instant::now();
        }
    }
    task.view.message = Some(format!("Verifying {}", item.target));
    progress(task.view.clone());
    let (reply, _) = peers
        .call(
            true,
            item.root,
            Command::Hash {
                path: item.source.clone(),
                stamp: item.stamp.clone(),
            },
            Vec::new(),
        )
        .await?;
    let Reply::Hash { digest } = reply else {
        return Err(ClientError::RemoteResponse);
    };
    peers
        .call(
            false,
            item.root,
            Command::Commit {
                path: item.target.clone(),
                token: item.token.clone(),
                digest,
            },
            Vec::new(),
        )
        .await?;
    task.view.message = None;
    Ok(())
}
