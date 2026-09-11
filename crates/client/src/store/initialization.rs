use crate::{ClientError, Result};
use nix::fcntl::{Flock, FlockArg};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    time::Duration,
};

/// Serialize first-open journal and schema setup across CLI/App processes.
/// SQLite transactions still protect ordinary operations after initialization.
pub(super) async fn lock(directory: &Path) -> Result<Flock<File>> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(directory.join("initialization.lock"))?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(ClientError::PrivatePath);
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => return Ok(lock),
            Err((returned, nix::errno::Errno::EWOULDBLOCK)) => {
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
