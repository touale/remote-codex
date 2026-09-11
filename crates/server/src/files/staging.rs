use super::io;
use crate::{Checked, Result};
use cap_std::fs::{Dir, File, OpenOptions, OpenOptionsExt};
use remote_codex_protocol::{Fault, MAX_TEXT_FILE};
use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

pub(super) struct Staged {
    pub owner: String,
    pub workspace: String,
    pub expires: Instant,
    parent: Dir,
    destination: PathBuf,
    temporary: PathBuf,
    file: File,
    expected: Option<String>,
    length: u64,
    written: u64,
}

impl Staged {
    pub(super) fn new(
        dir: &Dir,
        owner: &str,
        workspace: &str,
        path: &str,
        expected: Option<String>,
        length: u64,
    ) -> Result<Self> {
        if length > MAX_TEXT_FILE as u64 {
            return Err(Fault::new("FILE_TOO_LARGE", "File exceeds 4 MiB."));
        }
        let path = io::relative(path, false)?;
        let path = if dir.symlink_metadata(&path).is_ok() {
            dir.canonicalize(&path)
                .checked("FILE_PATH", "File must stay inside the workspace.")?
        } else {
            path
        };
        let parent = dir
            .open_dir(
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(std::path::Path::new(".")),
            )
            .checked("FILE_PATH", "Cannot open the destination directory.")?;
        let destination = PathBuf::from(
            path.file_name()
                .ok_or_else(|| Fault::new("FILE_PATH", "Invalid filename."))?,
        );
        if io::revision(&parent, &destination)? != expected {
            return Err(conflict());
        }
        let temporary = PathBuf::from(format!(".remote-codex-{}.tmp", uuid::Uuid::new_v4()));
        let file = parent
            .open_with(
                &temporary,
                OpenOptions::new().write(true).create_new(true).mode(0o600),
            )
            .checked("FILE_WRITE", "Cannot stage this file.")?;
        if let Ok(meta) = parent.metadata(&destination) {
            file.set_permissions(meta.permissions())
                .checked("FILE_WRITE", "Cannot preserve file permissions.")?;
        }
        Ok(Self {
            owner: owner.into(),
            workspace: workspace.into(),
            expires: Instant::now() + Duration::from_secs(120),
            parent,
            destination,
            temporary,
            file,
            expected,
            length,
            written: 0,
        })
    }
    pub(super) fn write(&mut self, offset: u64, bytes: &[u8]) -> Result<()> {
        if offset != self.written
            || bytes.len() > remote_codex_protocol::FILE_CHUNK
            || self.written + bytes.len() as u64 > self.length
        {
            return Err(Fault::new(
                "FILE_WRITE_SEQUENCE",
                "Upload chunk is out of sequence.",
            ));
        }
        self.file
            .write_all(bytes)
            .checked("FILE_WRITE", "Cannot write file contents.")?;
        self.written += bytes.len() as u64;
        self.expires = Instant::now() + Duration::from_secs(120);
        Ok(())
    }
    pub(super) fn commit(self) -> Result<String> {
        if self.written != self.length {
            return Err(Fault::new(
                "FILE_WRITE_INCOMPLETE",
                "File upload is incomplete.",
            ));
        }
        self.file
            .sync_all()
            .checked("FILE_WRITE", "Cannot flush file contents.")?;
        let (bytes, revision) = io::read(&self.parent, &self.temporary)?;
        std::str::from_utf8(&bytes)
            .map_err(|_| Fault::new("FILE_ENCODING", "The editor supports UTF-8 text files."))?;
        if io::revision(&self.parent, &self.destination)? != self.expected {
            return Err(conflict());
        }
        if self.expected.is_none() {
            // A concurrent creator must never be overwritten.
            self.parent
                .hard_link(&self.temporary, &self.parent, &self.destination)
                .checked(
                    "FILE_CONFLICT",
                    "The destination was created by another writer.",
                )?;
        } else {
            self.parent
                .rename(&self.temporary, &self.parent, &self.destination)
                .checked("FILE_WRITE", "Cannot replace this file.")?;
        }
        // Linux capability directories may use O_PATH, which cannot be fsynced.
        // Reopen the same anchored directory with a readable descriptor.
        self.parent.open_with(".", OpenOptions::new().read(true))
            .and_then(|directory| directory.sync_all())
            .map_err(|_| Fault::unknown("File was replaced but directory durability could not be confirmed. Reload before saving again."))?;
        Ok(revision)
    }
}
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = self.parent.remove_file(&self.temporary);
    }
}
pub(super) fn conflict() -> Fault {
    Fault::new(
        "FILE_CONFLICT",
        "This file changed remotely. Compare it with your edits before saving.",
    )
}
