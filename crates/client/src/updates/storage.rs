use super::{UpdateMode, error};
use crate::{Result, store::private_directory};
use remote_codex_platform::fs::{Creation, FileLock, LockMode, open_private};
use serde::{Deserialize, Serialize};
use std::fs::TryLockError;
use std::{io::Write, path::Path};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Saved {
    pub mode: UpdateMode,
    pub last_checked: Option<u64>,
    pub release: Option<super::release::Release>,
}

pub(super) fn lock(root: &Path, name: &str) -> Result<Option<FileLock>> {
    private_directory(root)?;
    let file = open_private(&root.join(name), Creation::IfMissing)?;
    match FileLock::try_lock(file, LockMode::Exclusive) {
        Ok(lock) => Ok(Some(lock)),
        Err((_, TryLockError::WouldBlock)) => Ok(None),
        Err((_, e)) => Err(std::io::Error::from(e).into()),
    }
}

pub(super) fn read(root: &Path) -> Result<Saved> {
    let file = match open_private(&root.join("state.json"), Creation::Existing) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Saved::default()),
        Err(e) => return Err(e.into()),
    };
    if file.metadata()?.len() > 4 * 1024 * 1024 {
        return Err(error("UPDATE_STATE", "Update state is too large."));
    }
    Ok(serde_json::from_reader(file)?)
}

pub(super) fn change(root: &Path, change: impl FnOnce(&mut Saved)) -> Result<()> {
    // State writes are brief and synchronous; downloads hold a separate operation lock.
    let file = open_private(&root.join("state.lock"), Creation::IfMissing)?;
    let _lock = FileLock::lock(file, LockMode::Exclusive)?;
    let mut state = read(root)?;
    change(&mut state);
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    file.write_all(&serde_json::to_vec(&state)?)?;
    file.as_file().sync_all()?;
    file.persist(root.join("state.json")).map_err(|e| e.error)?;
    Ok(())
}

pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
