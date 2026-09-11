//! Capability-scoped transfer filesystem shared by local and remote endpoints.
mod inspect;
mod journal;
mod paths;
mod staging;
use cap_std::fs::Dir;
use remote_codex_protocol::{
    Fault,
    transfer::{Command, Reply},
};
use std::path::Path;
pub type Result<T> = std::result::Result<T, Fault>;

pub struct Endpoint {
    root: Dir,
    owner: String,
    cancelled: std::sync::atomic::AtomicBool,
}
impl Endpoint {
    pub fn stop(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
    }
    pub fn identity(&self) -> Result<remote_codex_protocol::transfer::RootIdentity> {
        use cap_std::fs::MetadataExt;
        let metadata = self.root.dir_metadata().map_err(io)?;
        Ok(remote_codex_protocol::transfer::RootIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }

    pub fn open(root: &Path, owner: &str) -> Result<Self> {
        if !root.is_absolute() {
            return Err(fault("TRANSFER_PATH", "Transfer root must be absolute."));
        }
        Ok(Self {
            root: Dir::open_ambient_dir(root, cap_std::ambient_authority()).map_err(io)?,
            owner: owner.into(),
            cancelled: std::sync::atomic::AtomicBool::new(false),
        })
    }
    pub fn dispatch(&self, command: Command, bytes: &[u8]) -> Result<(Reply, Vec<u8>)> {
        check_cancelled(&self.cancelled)?;
        if !matches!(command, Command::Write { .. }) && !bytes.is_empty() {
            return Err(fault("TRANSFER_PACKET", "Unexpected transfer payload."));
        }
        let reply = match command {
            Command::Stat { path } => Reply::Stat {
                stamp: self.stat(&path)?,
            },
            Command::List { path, after } => Reply::List(self.list(&path, after.as_deref())?),
            Command::Read {
                path,
                stamp,
                offset,
            } => return self.read(&path, &stamp, offset),
            Command::Hash { path, stamp } => Reply::Hash {
                digest: self.hash(&path, &stamp)?,
            },
            Command::Prepare {
                path,
                token,
                source,
                expected,
            } => self.prepare(&path, &token, source, expected)?,
            Command::Write {
                path,
                token,
                offset,
                digest,
            } => self.write(&path, &token, offset, &digest, bytes)?,
            Command::Commit {
                path,
                token,
                digest,
            } => self.commit(&path, &token, &digest)?,
            Command::Cancel { path, token } => self.cancel(&path, &token)?,
            Command::Directory { path } => {
                let (dir, name) = self.parent(&path)?;
                dir.create_dir(&name).map_err(io)?;
                dir.into_std_file().sync_all().map_err(io)?;
                Reply::Done
            }
            Command::Metadata { path, stamp } => {
                self.metadata(&path, &stamp)?;
                Reply::Done
            }
        };
        Ok((reply, Vec::new()))
    }
}
fn io(error: impl std::fmt::Display) -> Fault {
    fault("TRANSFER_IO", &error.to_string())
}
fn fault(code: &str, message: &str) -> Fault {
    Fault::new(code, message)
}
pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn check_cancelled(cancelled: &std::sync::atomic::AtomicBool) -> Result<()> {
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        Err(fault("TRANSFER_CANCELLED", "Transfer paused."))
    } else {
        Ok(())
    }
}
