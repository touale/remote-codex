use crate::{ClientError, Result};
use nix::fcntl::{Flock, FlockArg};
use std::{fs::File, os::unix::fs::OpenOptionsExt, path::Path};
pub(crate) struct Lease {
    _file: Flock<File>,
}
impl Lease {
    pub(crate) fn acquire(directory: &Path, id: &str) -> Result<Self> {
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
        let file = Flock::lock(file, FlockArg::LockExclusiveNonblock)
            .map_err(|_| ClientError::Argument("This transfer is running in another window."))?;
        Ok(Self { _file: file })
    }
}
