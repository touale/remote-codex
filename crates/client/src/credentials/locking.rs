use crate::{ClientError, Result};
use remote_codex_platform::fs::{Creation, FileLock, LockMode, open_private};
use std::fs::TryLockError;
use std::{fs::File, path::Path, sync::Arc, time::Duration};

pub(crate) struct WritePermit {
    _lock: FileLock,
}

fn open(directory: &Path) -> Result<File> {
    let file = open_private(&directory.join("credentials.lock"), Creation::IfMissing)?;
    Ok(file)
}

/// Writers share a lock, so authenticating one server never serializes another.
pub(crate) async fn write(directory: &Path) -> Result<Arc<WritePermit>> {
    let mut file = open(directory)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match FileLock::try_lock(file, LockMode::Shared) {
            Ok(lock) => return Ok(Arc::new(WritePermit { _lock: lock })),
            Err((returned, TryLockError::WouldBlock)) => {
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
pub(super) fn cleanup(directory: &Path) -> Result<Option<FileLock>> {
    match FileLock::try_lock(open(directory)?, LockMode::Exclusive) {
        Ok(lock) => Ok(Some(lock)),
        Err((_, TryLockError::WouldBlock)) => Ok(None),
        Err((_, error)) => Err(std::io::Error::from(error).into()),
    }
}
