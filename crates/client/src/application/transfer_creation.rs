use super::transfers::TransferService;
use crate::{ClientError, Result, transfers::model::*};
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

impl TransferService {
    /// Local paths must be supplied by the host's native file authorization UI.
    pub async fn upload(
        &self,
        server: &str,
        workspace: &str,
        destination: &str,
        paths: Vec<PathBuf>,
    ) -> Result<Transfer> {
        relative(destination, true)?;
        if paths.is_empty() || paths.len() > 1000 {
            return Err(ClientError::Argument("Select between 1 and 1,000 entries."));
        }
        let mut grants = Vec::new();
        let mut sources = Vec::new();
        for path in paths {
            let parent = path
                .parent()
                .ok_or(ClientError::Argument(
                    "Select a file or folder inside a directory.",
                ))?
                .canonicalize()?;
            let name = path
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or(ClientError::Argument("File name is not supported."))?;
            relative(name, false)?;
            let root = match grants.iter().position(|g: &Grant| g.root == parent) {
                Some(root) => root,
                None => {
                    grants.push(grant(&parent)?);
                    grants.len() - 1
                }
            };
            if !sources
                .iter()
                .any(|s: &Source| s.root == root && s.path == name)
            {
                sources.push(Source {
                    root,
                    path: name.into(),
                });
            }
        }
        self.create(
            server,
            workspace,
            destination,
            Direction::Upload,
            grants,
            sources,
        )
        .await
    }
    /// Destination is an explicitly selected native directory, never a webview-provided path.
    pub async fn download(
        &self,
        server: &str,
        workspace: &str,
        source: &str,
        destination: PathBuf,
    ) -> Result<Transfer> {
        relative(source, false)?;
        let destination = destination.canonicalize()?;
        let display = destination.to_string_lossy().into_owned();
        self.create(
            server,
            workspace,
            &display,
            Direction::Download,
            vec![grant(&destination)?],
            vec![Source {
                root: 0,
                path: source.into(),
            }],
        )
        .await
    }
    async fn create(
        &self,
        server: &str,
        workspace: &str,
        destination: &str,
        direction: Direction,
        grants: Vec<Grant>,
        sources: Vec<Source>,
    ) -> Result<Transfer> {
        let state = &self.client.0;
        state.ensure_open()?;
        if !Path::new(workspace).is_absolute() {
            return Err(ClientError::Argument("Select a remote workspace."));
        }
        let record = state.store.find_connection(server).await?;
        let view = Transfer {
            id: uuid::Uuid::new_v4().to_string(),
            name: if sources.len() == 1 {
                sources[0].path.clone()
            } else {
                format!("{} items", sources.len())
            },
            server: record.name,
            workspace: workspace.into(),
            direction,
            destination: destination.into(),
            status: Status::Paused,
            bytes: 0,
            total: 0,
            files: 0,
            completed: 0,
            skipped: 0,
            message: None,
            error_code: None,
            conflict: None,
            active: false,
            owned: false,
        };
        let task = Task {
            view: view.clone(),
            server_id: record.id,
            grants,
            sources,
            remote_identity: None,
            remote_root: None,
            scanned: false,
            cancel: false,
            current: None,
            policy: None,
        };
        state.store.transfer_save(&task).await?;
        Ok(view)
    }
}
fn grant(root: &Path) -> Result<Grant> {
    let meta = std::fs::symlink_metadata(root)?;
    if !meta.is_dir() {
        return Err(ClientError::Argument("Select a local directory."));
    }
    Ok(Grant {
        root: root.into(),
        device: meta.dev(),
        inode: meta.ino(),
    })
}
fn relative(path: &str, empty: bool) -> Result<()> {
    if (!empty && path.is_empty())
        || path.len() > 4096
        || path.contains('\0')
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(ClientError::Argument("Select a path inside the workspace."));
    }
    Ok(())
}
