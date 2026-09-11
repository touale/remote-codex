use crate::{ClientError, Result};
use nix::fcntl::{Flock, FlockArg};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    sync::Mutex,
};

/// OS locks cover all frontends, including sessions before their first turn.
pub(crate) struct WorkspaceLock(Mutex<Option<Vec<Flock<File>>>>);

impl WorkspaceLock {
    pub(crate) fn acquire(
        root: &Path,
        server: &str,
        path: Option<&str>,
        exclusive: bool,
    ) -> Result<Self> {
        let directory = root.join("workspace-locks");
        crate::store::private_directory(&directory)?;
        let mut locks = Vec::new();
        for (key, write) in [
            (server.to_owned(), exclusive && path.is_none()),
            (format!("{server}\0{}", path.unwrap_or_default()), exclusive),
        ] {
            let name = format!("{:x}.lock", Sha256::digest(key.as_bytes()));
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .custom_flags(nix::libc::O_NOFOLLOW)
                .open(directory.join(name))?;
            let kind = if write {
                FlockArg::LockExclusiveNonblock
            } else {
                FlockArg::LockSharedNonblock
            };
            locks.push(Flock::lock(file, kind).map_err(|(_, error)| {
                if error == nix::errno::Errno::EWOULDBLOCK {
                    ClientError::RemoteFault("WORKSPACE_BUSY".into(), "Close the sessions, editors and terminals using this workspace, then retry.".into(), false)
                } else { std::io::Error::from(error).into() }
            })?);
            if path.is_none() {
                break;
            }
        }
        Ok(Self(Mutex::new(Some(locks))))
    }
    pub(crate) fn release(&self) {
        if let Ok(mut locks) = self.0.lock() {
            locks.take();
        }
    }
}
