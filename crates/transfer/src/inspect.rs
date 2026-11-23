use super::{Endpoint, Result, fault, io, paths};
#[cfg(windows)]
use cap_fs_ext::{MetadataExt, OsMetadataExt};
use cap_std::fs::Metadata;
#[cfg(unix)]
use cap_std::fs::MetadataExt;
use remote_codex_protocol::transfer::{CHUNK, Entry, Kind, Page, Reply, Stamp};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom};

fn stamp(m: &Metadata) -> Stamp {
    #[cfg(unix)]
    let linked = m.file_type().is_symlink();
    #[cfg(windows)]
    let linked = m.file_attributes() & 0x400 != 0; // FILE_ATTRIBUTE_REPARSE_POINT includes junctions.
    #[cfg(unix)]
    let (device, inode, mode) = (m.dev(), m.ino(), m.mode() & 0o777);
    #[cfg(windows)]
    let (device, inode, mode) = (
        m.dev(),
        m.ino(),
        if m.is_dir() {
            0o755
        } else if m.permissions().readonly() {
            0o444
        } else {
            0o644
        },
    );
    Stamp {
        kind: if linked {
            Kind::Unsupported
        } else if m.is_file() {
            Kind::File
        } else if m.is_dir() {
            Kind::Directory
        } else {
            Kind::Unsupported
        },
        length: m.len(),
        modified_ns: m
            .modified()
            .ok()
            .and_then(|t| t.into_std().duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
            .unwrap_or(0),
        device,
        inode,
        mode,
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
        let file = paths::file(&dir, &name, paths::Access::Read)?;
        let current = stamp(&Metadata::from_file(&file).map_err(io)?);
        if current != *expected {
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
            paths::file(&dir, &name, paths::Access::Read)?
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
    remote_codex_platform::fs::set_mode(file, stamp.mode & 0o777).map_err(io)?;
    file.set_times(
        std::fs::FileTimes::new().set_modified(
            std::time::UNIX_EPOCH + std::time::Duration::from_nanos(stamp.modified_ns),
        ),
    )
    .map_err(io)?;
    file.sync_all().map_err(io)
}
