//! Private storage, stable file identities and process-owned file locks.
use std::{fs::File, io, path::Path};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;
#[cfg(unix)]
use unix as native;
#[cfg(windows)]
use windows as native;

#[cfg(windows)]
pub use native::protect;
pub use native::{check_private, identity, mode, open_private, private_dir, set_mode};

pub fn identity_at(path: &Path) -> io::Result<FileIdentity> {
    let file = if std::fs::symlink_metadata(path)?.is_dir() {
        cap_std::fs::Dir::open_ambient_dir(path, cap_std::ambient_authority())?.into_std_file()
    } else {
        File::open(path)?
    };
    identity(&file)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    pub device: u64,
    pub inode: u64,
}

#[derive(Clone, Copy)]
pub enum Creation {
    Existing,
    IfMissing,
    New,
}

#[derive(Clone, Copy)]
pub enum LockMode {
    Shared,
    Exclusive,
}

/// The handle stays private so callers cannot clone it and extend lock ownership.
pub struct FileLock(File);

impl FileLock {
    pub fn try_lock(file: File, mode: LockMode) -> Result<Self, (File, std::fs::TryLockError)> {
        let result = match mode {
            LockMode::Shared => file.try_lock_shared(),
            LockMode::Exclusive => file.try_lock(),
        };
        match result {
            Ok(()) => Ok(Self(file)),
            Err(error) => Err((file, error)),
        }
    }

    pub fn lock(file: File, mode: LockMode) -> io::Result<Self> {
        match mode {
            LockMode::Shared => file.lock_shared()?,
            LockMode::Exclusive => file.lock()?,
        }
        Ok(Self(file))
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

pub fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        metadata.file_type().is_symlink()
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
}

pub fn sync_directory(directory: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        directory.sync_all()
    }
    #[cfg(windows)]
    {
        // Windows directory handles cannot be flushed; staged file data is synced
        // before the capability-relative rename publishes it.
        let _ = directory;
        Ok(())
    }
}

pub fn home_dir() -> io::Result<std::path::PathBuf> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(unix)]
    let home = std::env::var_os("HOME");
    home.map(Into::into)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "User home is unavailable"))
}

pub fn relative_posix(path: &Path) -> io::Result<String> {
    path.components()
        .map(|part| match part {
            std::path::Component::Normal(name) => name
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Path is not UTF-8")),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Expected a relative path",
            )),
        })
        .collect::<io::Result<Vec<_>>>()
        .map(|parts| parts.join("/"))
}
