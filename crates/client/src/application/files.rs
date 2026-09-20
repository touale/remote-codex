use super::ServerService;
use crate::{ClientError, Result, remote::Remote, workspace_lock::WorkspaceLock};
use remote_codex_protocol::{DirectoryPage, FileChunk, FileRequest, Request, TextFile};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Access to one remote directory, independent of saved workspaces.
#[derive(Clone)]
pub struct FileHandle(Arc<FileOwner>);
struct FileOwner {
    directory: std::path::PathBuf,
    remote: Arc<Remote>,
    path: String,
    lock: WorkspaceLock,
    closed: AtomicBool,
    operations: tokio::sync::RwLock<()>,
}
impl ServerService {
    pub async fn files(&self, server: &str, path: &str) -> Result<FileHandle> {
        FileHandle::open(&self.client, server, path).await
    }
}
impl FileHandle {
    pub(super) async fn open(client: &super::Client, server: &str, path: &str) -> Result<Self> {
        let state = &client.0;
        state.ensure_open()?;
        let record = state.store.find_connection(server).await?;
        let remote = state.connect(record).await?;
        if !remote
            .identity
            .capabilities
            .iter()
            .any(|v| v == remote_codex_protocol::WORKSPACE_FILES_CAPABILITY)
        {
            return Err(ClientError::RemoteFault(
                "SERVICE_UPDATE_REQUIRED".into(),
                "Reconnect after active work finishes to enable remote file access.".into(),
                false,
            ));
        }
        let result = remote
            .call(Request::Workspace { path: path.into() })
            .await?;
        let path = result["path"]
            .as_str()
            .ok_or(ClientError::RemoteResponse)?
            .to_owned();
        let lock = WorkspaceLock::acquire(&state.directory, &remote.server.id, Some(&path), false)?;
        Ok(Self(Arc::new(FileOwner {
            directory: state.directory.clone(),
            remote,
            path,
            lock,
            closed: AtomicBool::new(false),
            operations: tokio::sync::RwLock::new(()),
        })))
    }
    pub fn path(&self) -> &str {
        &self.0.path
    }
    pub fn server(&self) -> &str {
        &self.0.remote.server.name
    }
    pub fn server_id(&self) -> &str {
        &self.0.remote.server.id
    }
    pub fn close(&self) {
        self.0.closed.store(true, Ordering::Release);
        self.0.lock.release();
    }
    pub async fn shutdown(&self) {
        self.close();
        let _drained = self.0.operations.write().await;
    }
    pub async fn list(&self, path: &str) -> Result<DirectoryPage> {
        let _guard = self.operation_lock().await?;
        Ok(serde_json::from_value(
            self.call(FileRequest::List { path: path.into() }).await?,
        )?)
    }
    pub async fn read(&self, path: &str) -> Result<TextFile> {
        let _guard = self.operation_lock().await?;
        let mut bytes = Vec::new();
        let mut revision = None;
        loop {
            let chunk: FileChunk = serde_json::from_value(
                self.call(FileRequest::Read {
                    path: path.into(),
                    offset: bytes.len() as u64,
                    revision: revision.clone(),
                })
                .await?,
            )?;
            if chunk.length > remote_codex_protocol::MAX_TEXT_FILE as u64
                || chunk.bytes.len() > remote_codex_protocol::FILE_CHUNK
                || (chunk.bytes.is_empty() && bytes.len() as u64 != chunk.length)
            {
                return Err(ClientError::RemoteResponse);
            }
            revision = Some(chunk.revision);
            bytes.extend(chunk.bytes);
            if bytes.len() as u64 == chunk.length {
                break;
            }
            if bytes.len() as u64 > chunk.length {
                return Err(ClientError::RemoteResponse);
            }
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| ClientError::Argument("The editor supports UTF-8 text files."))?;
        if text.contains('\0') {
            return Err(ClientError::Argument(
                "Binary files cannot be opened in the text editor.",
            ));
        }
        Ok(TextFile {
            path: path.into(),
            text,
            revision: revision.ok_or(ClientError::RemoteResponse)?,
        })
    }
    /// Read a preview without exposing remote paths to the local WebView.
    pub async fn read_preview(&self, path: &str) -> Result<Vec<u8>> {
        let _guard = self.operation_lock().await?;
        self.0.remote.recover(false).await?;
        crate::transfers::preview::read(&self.0.remote, &self.0.path, path).await
    }
    pub async fn write(&self, path: &str, text: &str, revision: Option<String>) -> Result<String> {
        let _guard = self.operation_lock().await?;
        if text.len() > remote_codex_protocol::MAX_TEXT_FILE {
            return Err(ClientError::Argument("File exceeds 4 MiB."));
        }
        let opened = self
            .call(FileRequest::BeginWrite {
                path: path.into(),
                revision,
                length: text.len() as u64,
            })
            .await?;
        let token = opened["token"]
            .as_str()
            .ok_or(ClientError::RemoteResponse)?
            .to_owned();
        let result = async {
            for (index, bytes) in text
                .as_bytes()
                .chunks(remote_codex_protocol::FILE_CHUNK)
                .enumerate()
            {
                self.call(FileRequest::WriteChunk {
                    token: token.clone(),
                    offset: (index * remote_codex_protocol::FILE_CHUNK) as u64,
                    bytes: bytes.into(),
                })
                .await?;
            }
            let value = self
                .call(FileRequest::CommitWrite {
                    token: token.clone(),
                })
                .await?;
            value["revision"]
                .as_str()
                .map(str::to_owned)
                .ok_or(ClientError::RemoteResponse)
        }
        .await;
        if result.is_err() {
            let _ = self.call(FileRequest::CancelWrite { token }).await;
        }
        result
    }
    pub async fn create_directory(&self, path: &str) -> Result<()> {
        let _guard = self.operation_lock().await?;
        self.call(FileRequest::CreateDirectory { path: path.into() })
            .await?;
        Ok(())
    }
    pub async fn rename(&self, path: &str, destination: &str) -> Result<()> {
        let _guard = self.operation_lock().await?;
        self.call(FileRequest::Rename {
            path: path.into(),
            destination: destination.into(),
        })
        .await?;
        Ok(())
    }
    pub async fn remove(&self, path: &str) -> Result<()> {
        let _guard = self.operation_lock().await?;
        self.call(FileRequest::Remove { path: path.into() }).await?;
        Ok(())
    }
    pub async fn terminal(&self, columns: u16, rows: u16) -> Result<super::ShellHandle> {
        self.ensure_open()?;
        let (_activity, guard) = self.operation_lock().await?;
        self.0.remote.recover(false).await?;
        super::shell::open(self.0.remote.clone(), &self.0.path, columns, rows, guard).await
    }
    fn ensure_open(&self) -> Result<()> {
        if self.0.closed.load(Ordering::Acquire) {
            Err(ClientError::Argument("File context is closed."))
        } else {
            Ok(())
        }
    }
    async fn operation_lock(
        &self,
    ) -> Result<(tokio::sync::RwLockReadGuard<'_, ()>, WorkspaceLock)> {
        let activity = self.0.operations.read().await;
        let guard = WorkspaceLock::acquire(
            &self.0.directory,
            &self.0.remote.server.id,
            Some(&self.0.path),
            false,
        )?;
        self.ensure_open()?;
        Ok((activity, guard))
    }
    async fn call(&self, operation: FileRequest) -> Result<serde_json::Value> {
        if self.0.closed.load(Ordering::Acquire) {
            // Draining accepted work must not open a fresh authentication prompt.
            self.0.remote.recover_silent().await?;
        } else {
            self.0.remote.recover(false).await?;
        }
        self.0
            .remote
            .call(Request::WorkspaceFiles {
                workspace: self.0.path.clone(),
                operation,
            })
            .await
    }
}
