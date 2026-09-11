use super::{
    Endpoint, Result, fault, inspect, io,
    journal::{Journal, Stage, name},
    paths,
};
use remote_codex_protocol::transfer::{CHUNK, Kind, Reply, Stamp};
use rustix::fs::OFlags;
use std::io::{Seek, SeekFrom, Write};

impl Endpoint {
    pub(super) fn prepare(
        &self,
        path: &str,
        token: &str,
        source: Stamp,
        expected: Option<Stamp>,
    ) -> Result<Reply> {
        if source.kind != Kind::File {
            return Err(fault("TRANSFER_TYPE", "Only regular files can be staged."));
        }
        let initial = Journal {
            owner: self.owner.clone(),
            path: path.into(),
            source: source.clone(),
            expected: expected.clone(),
            offset: 0,
            digest: None,
            committed: false,
        };
        let mut stage = self.stage(path, token, Some(initial))?;
        if stage.journal.source != source || stage.journal.expected != expected {
            return Err(fault(
                "TRANSFER_SOURCE_CHANGED",
                "Transfer checkpoint describes a different source or destination.",
            ));
        }
        if stage.journal.digest.is_some() && stage.dir.symlink_metadata("data").is_err() {
            self.reconcile(&mut stage)?;
        }
        if !stage.journal.committed && self.stat(path)? != expected {
            return Err(fault(
                "TRANSFER_CONFLICT",
                "Destination changed. Review the conflict before continuing.",
            ));
        }
        if !stage.journal.committed {
            let length = paths::file(&stage.dir, "data", OFlags::RDONLY)?
                .metadata()
                .map_err(io)?
                .len();
            if length < stage.journal.offset {
                return Err(fault(
                    "TRANSFER_CHECKPOINT",
                    "Staged file is shorter than its durable checkpoint.",
                ));
            }
            if length > stage.journal.offset {
                stage.data()?.set_len(stage.journal.offset).map_err(io)?;
            }
        }
        Ok(Reply::Ready {
            offset: stage.journal.offset,
            complete: stage.journal.committed,
        })
    }
    pub(super) fn write(
        &self,
        path: &str,
        token: &str,
        offset: u64,
        digest: &str,
        bytes: &[u8],
    ) -> Result<Reply> {
        let mut stage = self.stage(path, token, None)?;
        if stage.journal.committed
            || bytes.is_empty()
            || bytes.len() > CHUNK
            || offset != stage.journal.offset
            || offset
                .checked_add(bytes.len() as u64)
                .is_none_or(|v| v > stage.journal.source.length)
            || super::digest(bytes) != digest
        {
            return Err(fault(
                "TRANSFER_CHECKPOINT",
                "Invalid transfer block or checkpoint.",
            ));
        }
        let mut file = stage.data()?;
        file.seek(SeekFrom::Start(offset)).map_err(io)?;
        file.write_all(bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
        stage.journal.offset += bytes.len() as u64;
        stage.save()?;
        Ok(Reply::Ready {
            offset: stage.journal.offset,
            complete: false,
        })
    }
    fn reconcile(&self, stage: &mut Stage) -> Result<()> {
        let stamp = self
            .stat(&stage.journal.path)?
            .ok_or_else(|| fault("TRANSFER_CONFLICT", "Committed destination is missing."))?;
        if stamp.kind != Kind::File
            || stamp.length != stage.journal.source.length
            || Some(self.hash(&stage.journal.path, &stamp)?) != stage.journal.digest
        {
            return Err(fault(
                "TRANSFER_CONFLICT",
                "Destination changed after transfer commit.",
            ));
        }
        stage.journal.committed = true;
        stage.save()
    }
    pub(super) fn commit(&self, path: &str, token: &str, digest: &str) -> Result<Reply> {
        let mut stage = self.stage(path, token, None)?;
        if stage.journal.digest.is_some() && stage.dir.symlink_metadata("data").is_err() {
            self.reconcile(&mut stage)?;
            return Ok(Reply::Done);
        }
        if stage.journal.offset != stage.journal.source.length
            || self.stat(path)? != stage.journal.expected
        {
            return Err(fault(
                "TRANSFER_CONFLICT",
                "Incomplete transfer or destination changed.",
            ));
        }
        let mut data = paths::file(&stage.dir, "data", OFlags::RDONLY)?;
        if inspect::hash_file(&mut data, &self.cancelled)? != digest {
            return Err(fault(
                "TRANSFER_CHECKSUM",
                "File verification failed. Restart this file.",
            ));
        }
        inspect::metadata(&data, &stage.journal.source)?;
        stage.journal.digest = Some(digest.into());
        stage.save()?;
        let (_, target) = self.parent(path)?;
        // Recheck immediately before publishing. No-replace makes new-name publication race safe.
        if self.stat(path)? != stage.journal.expected {
            return Err(fault(
                "TRANSFER_CONFLICT",
                "Destination changed before commit.",
            ));
        }
        if stage.journal.expected.is_none() {
            rustix::fs::renameat_with(
                &stage.dir,
                "data",
                &stage.parent,
                &target,
                rustix::fs::RenameFlags::NOREPLACE,
            )
            .map_err(io)?;
        } else {
            stage
                .dir
                .rename("data", &stage.parent, &target)
                .map_err(io)?;
        }
        stage
            .parent
            .try_clone()
            .map_err(io)?
            .into_std_file()
            .sync_all()
            .map_err(io)?;
        stage.journal.committed = true;
        stage.save()?;
        Ok(Reply::Done)
    }
    pub(super) fn cancel(&self, path: &str, token: &str) -> Result<Reply> {
        let (parent, _) = self.parent(path)?;
        let name = name(token)?;
        if parent
            .symlink_metadata(&name)
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            return Ok(Reply::Done);
        }
        let stage = self.stage(path, token, None)?;
        for file in ["data", "next.json", "state.json", "lock"] {
            match stage.dir.remove_file(file) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(e)),
            }
        }
        stage.parent.remove_dir(&stage.name).map_err(io)?;
        stage.parent.into_std_file().sync_all().map_err(io)?;
        Ok(Reply::Done)
    }
}
