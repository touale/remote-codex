use super::{Progress, lease::Lease, model::*, peer::Peers};
use crate::{ClientError, Result, remote::Remote, store::LocalStore};
use remote_codex_protocol::transfer::{Command, Kind};
use std::sync::Arc;

pub(crate) async fn publish(store: &LocalStore, task: &Task, progress: &Progress) -> Result<()> {
    store.transfer_save(task).await?;
    progress(task.view.clone());
    Ok(())
}
pub(crate) async fn run(
    store: &LocalStore,
    id: &str,
    remote: &Remote,
    lease: Arc<Lease>,
    progress: &Progress,
) -> Result<()> {
    let mut task = store.transfer(id).await?;
    if task
        .remote_identity
        .as_ref()
        .is_some_and(|old| old != &remote.identity.identity)
    {
        return Err(ClientError::Argument(
            "Server identity changed. Verify the server before restarting this transfer.",
        ));
    }
    if !remote
        .identity
        .capabilities
        .iter()
        .any(|c| c == remote_codex_protocol::transfer::CAPABILITY)
    {
        return Err(ClientError::Argument(
            "Finish active remote work and reconnect to install file transfer support.",
        ));
    }
    task.remote_identity = Some(remote.identity.identity.clone());
    task.view.active = true;
    task.view.owned = true;
    let mut peers = Peers::new(&task, remote, lease).await?;
    task.remote_root = Some(peers.remote.root.clone());
    if !task.scanned && !task.cancel {
        super::scan::scan(store, &mut task, &mut peers, progress).await?;
    }
    let mut items = store.transfer_items(id).await?;
    task.view.bytes = items.iter().map(|i| i.bytes).sum();
    task.view.completed = items.iter().filter(|i| i.done && !i.skipped).count();
    task.view.skipped = items.iter().filter(|i| i.skipped).count();
    task.view.status = Status::Running;
    task.view.message = None;
    publish(store, &task, progress).await?;
    if task.cancel {
        cleanup(&mut peers, &items).await?;
        task.view.status = Status::Cancelled;
        task.view.message = None;
        return publish(store, &task, progress).await;
    }
    for n in 0..items.len() {
        let mut item = items[n].clone();
        if item.done {
            continue;
        }
        task.current = Some(item.index);
        if item.target.is_empty() {
            let parent = item.parent.and_then(|index| items.get(index as usize));
            let base = parent.map(|i| i.target.as_str()).unwrap_or(
                if task.view.direction == Direction::Upload {
                    &task.view.destination
                } else {
                    ""
                },
            );
            item.target = join(
                base,
                item.source
                    .rsplit('/')
                    .next()
                    .ok_or(ClientError::RemoteResponse)?,
            );
            item.skipped =
                parent.is_some_and(|p| p.skipped) || item.stamp.kind == Kind::Unsupported;
        }
        if item.restart || item.restage {
            if item.prepared {
                peers
                    .call(
                        false,
                        item.root,
                        Command::Cancel {
                            path: item.target.clone(),
                            token: item.token.clone(),
                        },
                        Vec::new(),
                    )
                    .await?;
            }
            if item.restart {
                let previous_length = item.stamp.length;
                item.stamp = peers
                    .stat(true, item.root, &item.source)
                    .await?
                    .ok_or(ClientError::Argument("Source no longer exists."))?;
                if item.stamp.kind != Kind::File {
                    return Err(ClientError::Argument(
                        "Source is no longer a regular file. Start a new transfer.",
                    ));
                }
                task.view.total = task
                    .view
                    .total
                    .saturating_sub(previous_length)
                    .checked_add(item.stamp.length)
                    .ok_or(ClientError::Argument(
                        "Transfer size exceeds the supported range.",
                    ))?;
                item.choice = None;
            }
            item.token = uuid::Uuid::new_v4().to_string();
            task.view.bytes = task.view.bytes.saturating_sub(item.bytes);
            item.bytes = 0;
            item.prepared = false;
            item.restart = false;
            item.restage = false;
        }
        store.transfer_item(id, &item).await?;
        publish(store, &task, progress).await?;
        if !item.skipped && !super::conflict::destination(&mut task, &mut peers, &mut item).await? {
            store.transfer_item(id, &item).await?;
            return publish(store, &task, progress).await;
        }
        task.view.conflict = None;
        // Persist intent before any mutation so a lost reply can be reconciled.
        if !item.skipped {
            if item.stamp.kind == Kind::Directory {
                if !item.prepared && item.expected.is_none() {
                    item.prepared = true;
                    store.transfer_item(id, &item).await?;
                    peers
                        .call(
                            false,
                            item.root,
                            Command::Directory {
                                path: item.target.clone(),
                            },
                            Vec::new(),
                        )
                        .await?;
                } else if item.prepared
                    && peers.stat(false, item.root, &item.target).await?.is_none()
                {
                    peers
                        .call(
                            false,
                            item.root,
                            Command::Directory {
                                path: item.target.clone(),
                            },
                            Vec::new(),
                        )
                        .await?;
                }
            } else {
                store.transfer_item(id, &item).await?;
                if let Err(error) =
                    super::copy::file(store, &mut task, &mut peers, &mut item, progress).await
                {
                    if error.code() == "TRANSFER_CONFLICT"
                        && let Some(stamp) = peers.stat(false, item.root, &item.target).await?
                    {
                        item.choice = None;
                        task.view.status = Status::Conflict;
                        task.view.conflict = Some(Conflict {
                            path: item.target.clone(),
                            source: item.stamp.kind.clone(),
                            destination: stamp.kind.clone(),
                            expected: stamp,
                        });
                        store.transfer_item(id, &item).await?;
                        return publish(store, &task, progress).await;
                    }
                    return Err(error);
                }
            }
        }
        item.done = true;
        if item.skipped {
            task.view.skipped += 1;
        } else {
            task.view.completed += 1;
        }
        store.transfer_item(id, &item).await?;
        items[n] = item;
        publish(store, &task, progress).await?;
    }
    cleanup(&mut peers, &items).await?;
    for item in items
        .iter()
        .rev()
        .filter(|i| i.stamp.kind == Kind::Directory && !i.skipped && i.expected.is_none())
    {
        peers
            .call(
                false,
                item.root,
                Command::Metadata {
                    path: item.target.clone(),
                    stamp: item.stamp.clone(),
                },
                Vec::new(),
            )
            .await?;
    }
    task.view.status = Status::Completed;
    task.view.message = (task.view.skipped > 0).then(|| {
        format!(
            "{} entries skipped. Open details to review.",
            task.view.skipped
        )
    });
    task.current = None;
    publish(store, &task, progress).await
}
async fn cleanup(peers: &mut Peers, items: &[Item]) -> Result<()> {
    for item in items
        .iter()
        .filter(|i| i.prepared && i.stamp.kind == Kind::File)
    {
        let command = Command::Cancel {
            path: item.target.clone(),
            token: item.token.clone(),
        };
        if let Err(error) = peers
            .call(false, item.root, command.clone(), Vec::new())
            .await
        {
            if error.code() != "TRANSFER_MISSING" {
                return Err(error);
            }
            // Complete an interrupted initial journal before removing this task's stage.
            let prepared = peers
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
                .await;
            if let Err(error) = prepared
                && error.code() != "TRANSFER_CONFLICT"
            {
                return Err(error);
            }
            peers.call(false, item.root, command, Vec::new()).await?;
        }
    }
    Ok(())
}
