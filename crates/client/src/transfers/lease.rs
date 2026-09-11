use crate::{ClientError, Result};
use nix::fcntl::{Flock, FlockArg};
use std::{fs::File, os::unix::fs::OpenOptionsExt, path::Path};
pub(crate) struct Lease {
    _file: Flock<File>,
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
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(root.join(id))?;
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(file) => Ok(Some(Self { _file: file })),
            Err((_, nix::errno::Errno::EWOULDBLOCK)) => Ok(None),
            Err((_, error)) => Err(std::io::Error::from(error).into()),
        }
    }
}
