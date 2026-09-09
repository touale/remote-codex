use crate::{ClientError, Result};
use nix::fcntl::{Flock, FlockArg};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::Arc,
    time::Duration,
};

pub(crate) struct WritePermit {
    _lock: Flock<File>,
}

fn open(directory: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(directory.join("credentials.lock"))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(ClientError::PrivatePath);
    }
    Ok(file)
}

/// Writers share a lock, so authenticating one server never serializes another.
pub(crate) async fn write(directory: &Path) -> Result<Arc<WritePermit>> {
    let mut file = open(directory)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match Flock::lock(file, FlockArg::LockSharedNonblock) {
            Ok(lock) => return Ok(Arc::new(WritePermit { _lock: lock })),
            Err((returned, nix::errno::Errno::EWOULDBLOCK)) => {
                file = returned;
                if tokio::time::Instant::now() >= deadline {
                    return Err(ClientError::Credentials);
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err((_, error)) => return Err(std::io::Error::from(error).into()),
        }
    }
}

/// Maintenance never waits for an active credential writer.
pub(super) fn cleanup(directory: &Path) -> Result<Option<Flock<File>>> {
    match Flock::lock(open(directory)?, FlockArg::LockExclusiveNonblock) {
        Ok(lock) => Ok(Some(lock)),
        Err((_, nix::errno::Errno::EWOULDBLOCK)) => Ok(None),
        Err((_, error)) => Err(std::io::Error::from(error).into()),
    }
}
