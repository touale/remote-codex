use super::{UpdateMode, error};
use crate::{Result, store::private_directory};
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Saved {
    pub mode: UpdateMode,
    pub last_checked: Option<u64>,
    pub release: Option<super::release::Release>,
}

pub(super) fn lock(root: &Path, name: &str) -> Result<Option<Flock<File>>> {
    private_directory(root)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(root.join(name))?;
    match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
        Ok(lock) => Ok(Some(lock)),
        Err((_, nix::errno::Errno::EWOULDBLOCK)) => Ok(None),
        Err((_, e)) => Err(std::io::Error::from(e).into()),
    }
}

pub(super) fn read(root: &Path) -> Result<Saved> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(root.join("state.json"))
    {
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
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(root.join("state.lock"))?;
    let _lock =
        Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, e)| std::io::Error::from(e))?;
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
