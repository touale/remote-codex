mod io;
mod staging;
#[cfg(test)]
mod tests;
use crate::{Checked, Result};
use cap_std::fs::Dir;
use remote_codex_protocol::{FILE_CHUNK, Fault, FileChunk, FileRequest};
use serde_json::{Value, json};
use std::{collections::HashMap, time::Instant};

#[derive(Default)]
pub(crate) struct Files {
    stages: HashMap<String, staging::Staged>,
}

impl Files {
    pub(crate) fn dispatch(
        &mut self,
        owner: &str,
        workspace: &str,
        operation: &FileRequest,
    ) -> Result<Value> {
        self.stages
            .retain(|_, stage| stage.expires > Instant::now());
        if !std::path::Path::new(workspace).is_absolute() {
            return Err(Fault::new(
                "FILE_PATH",
                "Workspace must be an absolute remote directory.",
            ));
        }
        let dir = Dir::open_ambient_dir(workspace, cap_std::ambient_authority())
            .checked("FILE_PATH", "Cannot open this workspace.")?;
        match operation {
            FileRequest::List { path } => encode(io::list(&dir, path)?),
            FileRequest::Read {
                path,
                offset,
                revision,
            } => {
                let (bytes, actual) = io::read(&dir, &io::relative(path, false)?)?;
                if revision.as_ref().is_some_and(|old| old != &actual) {
                    return Err(staging::conflict());
                }
                let start = usize::try_from(*offset)
                    .ok()
                    .filter(|start| *start <= bytes.len())
                    .ok_or_else(|| Fault::new("FILE_OFFSET", "Invalid read offset."))?;
                let end = (start + FILE_CHUNK).min(bytes.len());
                encode(FileChunk {
                    bytes: bytes[start..end].into(),
                    revision: actual,
                    length: bytes.len() as u64,
                })
            }
            FileRequest::BeginWrite {
                path,
                revision,
                length,
            } => {
                if self.stages.len() >= 32 {
                    return Err(Fault::new(
                        "FILE_BUSY",
                        "Too many file uploads. Retry shortly.",
                    ));
                }
                let token = uuid::Uuid::new_v4().to_string();
                self.stages.insert(
                    token.clone(),
                    staging::Staged::new(&dir, owner, workspace, path, revision.clone(), *length)?,
                );
                Ok(json!({"token":token}))
            }
            FileRequest::WriteChunk {
                token,
                offset,
                bytes,
            } => {
                self.stage(owner, workspace, token)?.write(*offset, bytes)?;
                Ok(json!({}))
            }
            FileRequest::CommitWrite { token } => {
                self.stage(owner, workspace, token)?;
                let stage = self.stages.remove(token).ok_or_else(missing)?;
                Ok(json!({"revision":stage.commit()?}))
            }
            FileRequest::CancelWrite { token } => {
                self.stage(owner, workspace, token)?;
                self.stages.remove(token);
                Ok(json!({}))
            }
            FileRequest::CreateDirectory { path } => {
                dir.create_dir(io::relative(path, false)?)
                    .checked("FILE_CREATE", "Cannot create the directory.")?;
                Ok(json!({}))
            }
            FileRequest::Remove { path } => {
                let path = io::relative(path, false)?;
                let meta = dir
                    .symlink_metadata(&path)
                    .checked("FILE_REMOVE", "Cannot inspect this entry.")?;
                if meta.is_dir() {
                    dir.remove_dir(path)
                } else {
                    dir.remove_file(path)
                }
                .checked(
                    "FILE_REMOVE",
                    "Cannot remove this entry. Directories must be empty.",
                )?;
                Ok(json!({}))
            }
            FileRequest::Rename { path, destination } => {
                let path = io::relative(path, false)?;
                let destination = io::relative(destination, false)?;
                if dir.symlink_metadata(&destination).is_ok() {
                    return Err(staging::conflict());
                }
                let source_parent = dir
                    .open_dir(
                        path.parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .unwrap_or(std::path::Path::new(".")),
                    )
                    .checked("FILE_RENAME", "Cannot open the source directory.")?;
                let target_parent = dir
                    .open_dir(
                        destination
                            .parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .unwrap_or(std::path::Path::new(".")),
                    )
                    .checked("FILE_RENAME", "Cannot open the destination directory.")?;
                let source = path.file_name().ok_or_else(missing)?;
                let target = destination.file_name().ok_or_else(missing)?;
                rustix::fs::renameat_with(
                    &source_parent,
                    source,
                    &target_parent,
                    target,
                    rustix::fs::RenameFlags::NOREPLACE,
                )
                .checked(
                    "FILE_RENAME",
                    "Cannot rename this entry without replacing another file.",
                )?;
                Ok(json!({}))
            }
        }
    }
    fn stage(&mut self, owner: &str, workspace: &str, token: &str) -> Result<&mut staging::Staged> {
        self.stages
            .get_mut(token)
            .filter(|s| s.owner == owner && s.workspace == workspace)
            .ok_or_else(missing)
    }
}
fn missing() -> Fault {
    Fault::new(
        "FILE_UPLOAD_EXPIRED",
        "File upload expired or belongs to another workspace.",
    )
}
fn encode(value: impl serde::Serialize) -> Result<Value> {
    serde_json::to_value(value).checked("FILE_RESPONSE", "Cannot encode the file response.")
}
