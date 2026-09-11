use super::{Endpoint, Result, fault, io, paths};
use cap_std::fs::Dir;
use nix::fcntl::{Flock, FlockArg};
use remote_codex_protocol::transfer::Stamp;
use rustix::fs::OFlags;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    os::unix::fs::PermissionsExt,
};

#[derive(Serialize, Deserialize)]
pub(super) struct Journal {
    pub(super) owner: String,
    pub(super) path: String,
    pub(super) source: Stamp,
    pub(super) expected: Option<Stamp>,
    pub(super) offset: u64,
    pub(super) digest: Option<String>,
    pub(super) committed: bool,
}
pub(super) struct Stage {
    pub(super) parent: Dir,
    pub(super) name: String,
    pub(super) dir: Dir,
    pub(super) journal: Journal,
    _lock: Flock<std::fs::File>,
}
impl Stage {
    pub(super) fn save(&self) -> Result<()> {
        let mut file = paths::file(
            &self.dir,
            "next.json",
            OFlags::WRONLY | OFlags::CREATE | OFlags::TRUNC,
        )?;
        file.write_all(&serde_json::to_vec(&self.journal).map_err(io)?)
            .map_err(io)?;
        file.sync_all().map_err(io)?;
        self.dir
            .rename("next.json", &self.dir, "state.json")
            .map_err(io)?;
        self.dir
            .try_clone()
            .map_err(io)?
            .into_std_file()
            .sync_all()
            .map_err(io)
    }
    pub(super) fn data(&self) -> Result<std::fs::File> {
        paths::file(&self.dir, "data", OFlags::RDWR)
    }
}
pub(super) fn name(token: &str) -> Result<String> {
    let id = uuid::Uuid::parse_str(token)
        .map_err(|_| fault("TRANSFER_TOKEN", "Invalid transfer token."))?;
    Ok(format!(".remote-codex-transfer-{id}"))
}
impl Endpoint {
    pub(super) fn stage(&self, path: &str, token: &str, initial: Option<Journal>) -> Result<Stage> {
        let (parent, _) = self.parent(path)?;
        let name = name(token)?;
        let created = if initial.is_none() {
            false
        } else {
            match parent.create_dir(&name) {
                Ok(()) => true,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => false,
                Err(e) => return Err(io(e)),
            }
        };
        let dir = paths::directory(&parent, &name)?;
        if created {
            dir.set_permissions(
                ".",
                cap_std::fs::Permissions::from_std(std::fs::Permissions::from_mode(0o700)),
            )
            .map_err(io)?;
        }
        let lock = paths::file(&dir, "lock", OFlags::RDWR | OFlags::CREATE)?;
        let lock = Flock::lock(lock, FlockArg::LockExclusiveNonblock)
            .map_err(|_| fault("TRANSFER_BUSY", "Another transfer owns this staging file."))?;
        let missing = dir
            .symlink_metadata("state.json")
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
        let initialize = missing && initial.is_some();
        let journal = match paths::file(&dir, "state.json", OFlags::RDONLY) {
            Ok(file) => serde_json::from_reader(file.take(16384)).map_err(io)?,
            Err(_) if initialize => initial
                .ok_or_else(|| fault("TRANSFER_MISSING", "Transfer checkpoint is unavailable."))?,
            Err(_) if missing => {
                return Err(fault(
                    "TRANSFER_MISSING",
                    "Transfer initialization was interrupted.",
                ));
            }
            Err(error) => return Err(error),
        };
        if journal.owner != self.owner || journal.path != path {
            return Err(fault(
                "TRANSFER_OWNER",
                "Transfer belongs to another destination or owner.",
            ));
        }
        let stage = Stage {
            parent,
            name,
            dir,
            journal,
            _lock: lock,
        };
        if initialize {
            // There is no durable checkpoint yet; an interrupted first block may be discarded.
            paths::file(
                &stage.dir,
                "data",
                OFlags::RDWR | OFlags::CREATE | OFlags::TRUNC,
            )?
            .sync_all()
            .map_err(io)?;
            stage.save()?;
            stage
                .parent
                .try_clone()
                .map_err(io)?
                .into_std_file()
                .sync_all()
                .map_err(io)?;
        }
        Ok(stage)
    }
}
