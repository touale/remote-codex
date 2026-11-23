use crate::{ClientError, Result};
use remote_codex_platform::fs::{Creation, FileLock, LockMode, open_private};
use std::fs::TryLockError;
use std::{path::Path, time::Duration};

/// Serialize first-open journal and schema setup across CLI/App processes.
/// SQLite transactions still protect ordinary operations after initialization.
pub(super) async fn lock(directory: &Path) -> Result<FileLock> {
    let mut file = open_private(&directory.join("initialization.lock"), Creation::IfMissing)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match FileLock::try_lock(file, LockMode::Exclusive) {
            Ok(lock) => return Ok(lock),
            Err((returned, TryLockError::WouldBlock)) => {
                file = returned;
                if tokio::time::Instant::now() >= deadline {
                    return Err(ClientError::Timeout);
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err((_, error)) => return Err(std::io::Error::from(error).into()),
        }
    }
}
