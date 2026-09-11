use super::{model::*, peer::Peers};
use crate::{ClientError, Result};
use remote_codex_protocol::transfer::Kind;

pub(super) async fn destination(
    task: &mut Task,
    peers: &mut Peers,
    item: &mut Item,
) -> Result<bool> {
    if item.prepared {
        return Ok(true);
    }
    let existing = peers.stat(false, item.root, &item.target).await?;
    let Some(stamp) = existing else {
        item.expected = None;
        return Ok(true);
    };
    let choice = item.choice.or(task.policy).filter(|choice| match choice {
        Choice::Replace => stamp.kind == Kind::File && item.stamp.kind == Kind::File,
        Choice::Merge => stamp.kind == Kind::Directory && item.stamp.kind == Kind::Directory,
        _ => true,
    });
    if let Some(conflict) = &task.view.conflict
        && item.choice.is_some()
        && conflict.expected != stamp
    {
        item.choice = None;
        task.view.conflict = None;
        return ask(task, item, stamp);
    }
    match choice {
        Some(Choice::Skip) => {
            item.skipped = true;
            Ok(true)
        }
        Some(Choice::Replace | Choice::Merge) => {
            item.expected = Some(stamp);
            Ok(true)
        }
        Some(Choice::KeepBoth) => {
            let original = item.target.clone();
            for suffix in 2..10_000 {
                item.target = alternate(&original, suffix, item.stamp.kind == Kind::Directory);
                if peers.stat(false, item.root, &item.target).await?.is_none() {
                    item.expected = None;
                    return Ok(true);
                }
            }
            Err(ClientError::Argument(
                "Could not find an unused destination name.",
            ))
        }
        None => ask(task, item, stamp),
    }
}
fn ask(
    task: &mut Task,
    item: &Item,
    stamp: remote_codex_protocol::transfer::Stamp,
) -> Result<bool> {
    task.view.status = Status::Conflict;
    task.view.conflict = Some(Conflict {
        path: item.target.clone(),
        source: item.stamp.kind.clone(),
        destination: stamp.kind.clone(),
        expected: stamp,
    });
    Ok(false)
}
fn alternate(path: &str, number: usize, directory: bool) -> String {
    let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
    let (stem, extension) = if directory {
        (name, None)
    } else {
        name.rsplit_once('.')
            .filter(|(s, _)| !s.is_empty())
            .map_or((name, None), |(s, e)| (s, Some(e)))
    };
    join(
        parent,
        &format!(
            "{stem} ({number}){}",
            extension.map(|e| format!(".{e}")).unwrap_or_default()
        ),
    )
}
