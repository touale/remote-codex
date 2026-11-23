use crate::{ClientError, Result};
use remote_codex_platform::fs::{Creation, FileLock, LockMode, open_private};
use sha2::{Digest, Sha256};
use std::fs::TryLockError;
use std::{path::Path, sync::Mutex};

/// OS locks cover all frontends, including sessions before their first turn.
pub(crate) struct WorkspaceLock(Mutex<Option<Vec<FileLock>>>);

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
            let file = open_private(&directory.join(name), Creation::IfMissing)?;
            let kind = if write {
                LockMode::Exclusive
            } else {
                LockMode::Shared
            };
            locks.push(FileLock::try_lock(file, kind).map_err(|(_, error)| {
                if matches!(error, TryLockError::WouldBlock) {
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
