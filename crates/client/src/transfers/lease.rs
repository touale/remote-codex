use crate::{ClientError, Result};
use remote_codex_platform::fs::{Creation, FileLock, LockMode, open_private};
use std::fs::TryLockError;
use std::path::Path;
pub(crate) struct Lease {
    _file: FileLock,
}
impl Lease {
    pub(crate) fn acquire(directory: &Path, id: &str) -> Result<Self> {
        Self::try_acquire(directory, id)?.ok_or(ClientError::Argument(
            "This transfer is running in another window.",
        ))
    }
    pub(crate) fn try_acquire(directory: &Path, id: &str) -> Result<Option<Self>> {
        uuid::Uuid::parse_str(id).map_err(|_| ClientError::Argument("Invalid transfer ID."))?;
        let root = directory.join("transfer-locks");
        crate::store::private_directory(&root)?;
        let file = open_private(&root.join(id), Creation::IfMissing)?;
        match FileLock::try_lock(file, LockMode::Exclusive) {
            Ok(file) => Ok(Some(Self { _file: file })),
            Err((_, TryLockError::WouldBlock)) => Ok(None),
            Err((_, error)) => Err(std::io::Error::from(error).into()),
        }
    }
}
