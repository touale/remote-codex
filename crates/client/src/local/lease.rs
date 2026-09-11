use crate::{ClientError, Result, store::private_directory};
use nix::fcntl::{Flock, FlockArg};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::sync::watch;

/// Locks are OS-owned; takeover asks the existing frontend to detach, and only
/// succeeds after it releases its lock. No PID-based process termination.
pub(crate) struct Lease {
    lock: std::sync::Mutex<Option<Flock<File>>>,
    marker: PathBuf,
    generation: String,
    pub(crate) revoked: watch::Receiver<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl Lease {
    pub(crate) async fn acquire(directory: &Path, thread: &str, takeover: bool) -> Result<Self> {
        let root = directory.join("leases");
        private_directory(&root)?;
        let key = format!("{:x}", Sha256::digest(thread.as_bytes()));
        let path = root.join(format!("{key}.lock"));
        let marker = root.join(format!("{key}.request"));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(path)?;
        let mut file = file;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
        let generation = uuid::Uuid::new_v4().to_string();
        let mut requested = false;
        let lock = loop {
            match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                Ok(lock) => break lock,
                Err((returned, nix::errno::Errno::EWOULDBLOCK)) => {
                    file = returned;
                    if !takeover {
                        return Err(ClientError::SessionInUse);
                    }
                    if !requested {
                        let stage = tempfile::NamedTempFile::new_in(&root)?;
                        std::fs::write(stage.path(), &generation)?;
                        stage.persist(&marker).map_err(|e| e.error)?;
                        requested = true;
                    }
                    if tokio::time::Instant::now() >= deadline {
                        return Err(ClientError::Timeout);
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err((_, error)) => return Err(std::io::Error::from(error).into()),
            }
        };
        let initial = std::fs::read_to_string(&marker).unwrap_or_default();
        let watch_path = marker.clone();
        let (changed, revoked) = watch::channel(false);
        let task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if std::fs::read_to_string(&watch_path).unwrap_or_default() != initial {
                    let _ = changed.send(true);
                    break;
                }
            }
        });
        Ok(Self {
            lock: std::sync::Mutex::new(Some(lock)),
            marker,
            generation,
            revoked,
            task,
        })
    }
    pub(crate) fn release(&self) {
        if let Ok(mut lock) = self.lock.lock()
            && lock.is_some()
        {
            self.task.abort();
            if std::fs::read_to_string(&self.marker).ok().as_deref() == Some(&self.generation) {
                let _ = std::fs::remove_file(&self.marker);
            }
            lock.take();
        }
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.release();
    }
}
