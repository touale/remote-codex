use super::{Endpoint, Result, fault, io, paths};
use cap_std::fs::{Metadata, MetadataExt as CapMetadataExt};
use remote_codex_protocol::transfer::{CHUNK, Entry, Kind, Page, Reply, Stamp};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
    os::unix::fs::{MetadataExt, PermissionsExt},
};

fn stamp(m: &Metadata) -> Stamp {
    Stamp {
        kind: if m.is_file() {
            Kind::File
        } else if m.is_dir() {
            Kind::Directory
        } else {
            Kind::Unsupported
        },
        length: m.len(),
        modified_ns: (m.mtime() as u64)
            .saturating_mul(1_000_000_000)
            .saturating_add(m.mtime_nsec() as u64),
        device: m.dev(),
        inode: m.ino(),
        mode: m.mode() & 0o777,
    }
}
impl Endpoint {
    pub(super) fn stat(&self, path: &str) -> Result<Option<Stamp>> {
        let (dir, name) = if path.is_empty() {
            (self.root.try_clone().map_err(io)?, ".".into())
        } else {
            self.parent(path)?
        };
        match dir.symlink_metadata(name) {
            Ok(m) => Ok(Some(stamp(&m))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(e)),
        }
    }
    pub(super) fn list(&self, path: &str, after: Option<&str>) -> Result<Page> {
        let dir = self.directory(path)?;
        // Bounded lexicographic page, independent of directory size and platform enumeration order.
        let mut names = std::collections::BTreeSet::new();
        for entry in dir.entries().map_err(io)? {
            super::check_cancelled(&self.cancelled)?;
            let name = entry
                .map_err(io)?
                .file_name()
                .into_string()
                .map_err(|_| fault("TRANSFER_NAME", "Directory contains a non-UTF-8 name."))?;
            let internal = name
                .strip_prefix(".remote-codex-transfer-")
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
                && paths::directory(&dir, &name).is_ok_and(|stage| {
                    stage
                        .symlink_metadata("state.json")
                        .is_ok_and(|m| m.is_file())
                });
            if internal || after.is_some_and(|a| name.as_str() <= a) {
                continue;
            }
            names.insert(name);
            if names.len() > 101 {
                names.pop_last();
            }
        }
        let more = names.len() > 100;
        if more {
            names.pop_last();
        }
        let next = if more { names.last().cloned() } else { None };
        let mut entries = Vec::new();
        for name in names {
            let stamp = stamp(&dir.symlink_metadata(&name).map_err(io)?);
            entries.push(Entry { name, stamp });
        }
        Ok(Page {
            entries,
            after: next,
        })
    }
    fn source(&self, path: &str, expected: &Stamp) -> Result<std::fs::File> {
        let (dir, name) = self.parent(path)?;
        let file = paths::file(&dir, &name, rustix::fs::OFlags::RDONLY)?;
        let m = file.metadata().map_err(io)?;
        let modified = (m.mtime() as u64)
            .saturating_mul(1_000_000_000)
            .saturating_add(m.mtime_nsec() as u64);
        if m.len() != expected.length
            || m.ino() != expected.inode
            || m.dev() != expected.device
            || modified != expected.modified_ns
        {
            return Err(fault(
                "TRANSFER_SOURCE_CHANGED",
                "Source changed. Restart this file to transfer its new contents.",
            ));
        }
        Ok(file)
    }
    pub(super) fn read(
        &self,
        path: &str,
        expected: &Stamp,
        offset: u64,
    ) -> Result<(Reply, Vec<u8>)> {
        if offset > expected.length {
            return Err(fault("TRANSFER_OFFSET", "Invalid source offset."));
        }
        let mut file = self.source(path, expected)?;
        file.seek(SeekFrom::Start(offset)).map_err(io)?;
        let mut bytes = vec![0; CHUNK.min((expected.length - offset) as usize)];
        file.read_exact(&mut bytes).map_err(io)?;
        self.source(path, expected)?;
        Ok((
            Reply::Data {
                digest: super::digest(&bytes),
            },
            bytes,
        ))
    }
    pub(super) fn hash(&self, path: &str, expected: &Stamp) -> Result<String> {
        let mut file = self.source(path, expected)?;
        let result = hash_file(&mut file, &self.cancelled)?;
        self.source(path, expected)?;
        Ok(result)
    }
    pub(super) fn metadata(&self, path: &str, stamp: &Stamp) -> Result<()> {
        let file = if stamp.kind == Kind::Directory {
            self.directory(path)?.into_std_file()
        } else {
            let (dir, name) = self.parent(path)?;
            paths::file(&dir, &name, rustix::fs::OFlags::RDONLY)?
        };
        metadata(&file, stamp)
    }
}
pub(super) fn hash_file(
    file: &mut std::fs::File,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<String> {
    file.seek(SeekFrom::Start(0)).map_err(io)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; CHUNK];
    loop {
        super::check_cancelled(cancelled)?;
        let count = file.read(&mut buffer).map_err(io)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
pub(super) fn metadata(file: &std::fs::File, stamp: &Stamp) -> Result<()> {
    file.set_permissions(std::fs::Permissions::from_mode(stamp.mode & 0o777))
        .map_err(io)?;
    file.set_times(
        std::fs::FileTimes::new().set_modified(
            std::time::UNIX_EPOCH + std::time::Duration::from_nanos(stamp.modified_ns),
        ),
    )
    .map_err(io)?;
    file.sync_all().map_err(io)
}
