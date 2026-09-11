use super::{Client, FileHandle};
use crate::{Result, workspace_lock::WorkspaceLock};
pub use remote_codex_core::workspace::Workspace;

pub struct WorkspaceService {
    pub(super) client: Client,
}
#[derive(Clone)]
pub struct WorkspaceHandle(FileHandle);
impl WorkspaceService {
    pub async fn list(&self) -> Result<Vec<Workspace>> {
        self.client.0.store.workspace_catalog().await
    }
    pub async fn open(&self, server: &str, path: &str) -> Result<WorkspaceHandle> {
        let files = FileHandle::open(&self.client, server, path).await?;
        self.client
            .0
            .store
            .remember_workspace(files.server_id(), files.path())
            .await?;
        Ok(WorkspaceHandle(files))
    }
    pub async fn remove(&self, server: &str, path: &str) -> Result<()> {
        let state = &self.client.0;
        let record = state.store.find_connection(server).await?;
        let _lock = WorkspaceLock::acquire(&state.directory, &record.id, Some(path), true)?;
        state.store.remove_workspace(&record.id, path).await
    }
}
impl WorkspaceHandle {
    pub fn files(&self) -> &FileHandle {
        &self.0
    }
    pub fn path(&self) -> &str {
        self.0.path()
    }
    pub fn server(&self) -> &str {
        self.0.server()
    }
    pub fn close(&self) {
        self.0.close();
    }
    pub async fn terminal(&self, columns: u16, rows: u16) -> Result<super::ShellHandle> {
        self.0.terminal(columns, rows).await
    }
}
