use super::{Progress, engine::publish, model::*, peer::Peers};
use crate::{ClientError, Result, store::LocalStore};
use remote_codex_protocol::transfer::{Command, Kind, Reply};

pub(super) async fn scan(
    store: &LocalStore,
    task: &mut Task,
    peers: &mut Peers,
    progress: &Progress,
) -> Result<()> {
    store.transfer_reset_scan(&task.view.id).await?;
    task.view.status = Status::Preparing;
    task.view.total = 0;
    task.view.files = 0;
    publish(store, task, progress).await?;
    let mut pending = task
        .sources
        .iter()
        .rev()
        .map(|s| (s.root, s.path.clone(), None))
        .collect::<Vec<_>>();
    let mut index = 0i64;
    while let Some((root, path, parent)) = pending.pop() {
        if index >= 100_000 {
            return Err(ClientError::Argument(
                "Transfer exceeds 100,000 entries. Select smaller folders.",
            ));
        }
        let stamp = peers
            .stat(true, root, &path)
            .await?
            .ok_or(ClientError::Argument("Source no longer exists."))?;
        let item = Item {
            index,
            parent,
            root,
            source: path.clone(),
            target: String::new(),
            stamp: stamp.clone(),
            token: uuid::Uuid::new_v4().to_string(),
            expected: None,
            prepared: false,
            bytes: 0,
            done: false,
            skipped: false,
            choice: None,
            restart: false,
            restage: false,
        };
        store.transfer_item(&task.view.id, &item).await?;
        if stamp.kind == Kind::File {
            task.view.total =
                task.view
                    .total
                    .checked_add(stamp.length)
                    .ok_or(ClientError::Argument(
                        "Transfer size exceeds the supported range.",
                    ))?;
        }
        task.view.files += 1;
        if stamp.kind == Kind::Directory {
            let mut after = None;
            let mut children = Vec::new();
            loop {
                let (reply, _) = peers
                    .call(
                        true,
                        root,
                        Command::List {
                            path: path.clone(),
                            after,
                        },
                        Vec::new(),
                    )
                    .await?;
                let Reply::List(page) = reply else {
                    return Err(ClientError::RemoteResponse);
                };
                for entry in page.entries {
                    children.push((root, join(&path, &entry.name), Some(index)));
                }
                after = page.after;
                if after.is_none() {
                    break;
                }
                if pending.len() + children.len() > 100_000 {
                    return Err(ClientError::Argument(
                        "Directory is too large. Select smaller folders.",
                    ));
                }
            }
            pending.extend(children.into_iter().rev());
        }
        index += 1;
        if index % 100 == 0 {
            publish(store, task, progress).await?;
        }
    }
    task.scanned = true;
    publish(store, task, progress).await
}
