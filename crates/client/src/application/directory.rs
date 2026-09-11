use super::WorkspaceService;
use crate::{ClientError, Result, remote::Remote, workspace_lock::WorkspaceLock};
use remote_codex_protocol::{DirectoryPage, FileRequest, Request};
use std::sync::{Arc, Mutex};

/// Owns a connection while choosing a workspace, without adding a catalog entry.
#[derive(Clone)]
pub struct DirectoryBrowser(Arc<BrowserOwner>);
struct BrowserOwner {
    server: String,
    remote: Mutex<Option<Arc<Remote>>>,
    lock: WorkspaceLock,
}

impl WorkspaceService {
    pub async fn browser(&self, server: &str) -> Result<DirectoryBrowser> {
        let state = &self.client.0;
        state.ensure_open()?;
        let record = state.store.find_connection(server).await?;
        let lock = WorkspaceLock::acquire(&state.directory, &record.id, None, false)?;
        let remote = state.connect(record).await?;
        Ok(DirectoryBrowser(Arc::new(BrowserOwner {
            server: remote.server.name.clone(),
            remote: Mutex::new(Some(remote)),
            lock,
        })))
    }
}

impl DirectoryBrowser {
    pub fn server(&self) -> &str {
        &self.0.server
    }
    pub fn close(&self) {
        if let Ok(mut remote) = self.0.remote.lock() {
            remote.take();
        }
        self.0.lock.release();
    }
    pub async fn list(&self, path: &str) -> Result<DirectoryPage> {
        let remote = self.remote()?;
        let path = if path.is_empty() {
            &remote.identity.home
        } else {
            path
        };
        Ok(serde_json::from_value(
            self.call(
                path,
                FileRequest::List {
                    path: String::new(),
                },
            )
            .await?,
        )?)
    }
    pub async fn create_directory(&self, parent: &str, name: &str) -> Result<String> {
        validate_child(parent, name)?;
        self.call(parent, FileRequest::CreateDirectory { path: name.into() })
            .await?;
        Ok(format!("{}/{name}", parent.trim_end_matches('/')))
    }
    fn remote(&self) -> Result<Arc<Remote>> {
        self.0
            .remote
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .clone()
            .ok_or(ClientError::Argument("Directory browser is closed."))
    }
    async fn call(&self, path: &str, operation: FileRequest) -> Result<serde_json::Value> {
        if !path.starts_with('/') || path.chars().any(char::is_control) {
            return Err(ClientError::Argument(
                "Choose an absolute remote directory.",
            ));
        }
        let remote = self.remote()?;
        remote.recover(false).await?;
        self.remote()?;
        remote
            .call(Request::WorkspaceFiles {
                workspace: path.into(),
                operation,
            })
            .await
    }
}

fn validate_child(parent: &str, name: &str) -> Result<()> {
    if !parent.starts_with('/') || parent.chars().any(char::is_control) {
        return Err(ClientError::Argument(
            "Choose an absolute remote directory.",
        ));
    }
    if name.trim().is_empty()
        || matches!(name, "." | "..")
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err(ClientError::Argument(
            "Enter a single folder name without slashes or control characters.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_single_child_names_are_accepted() {
        for (parent, name) in [
            ("relative", "project"),
            ("/tmp\n", "project"),
            ("/tmp", ""),
            ("/tmp", "  "),
            ("/tmp", "."),
            ("/tmp", ".."),
            ("/tmp", "../escape"),
            ("/tmp", "nested/child"),
            ("/tmp", "nested\\child"),
            ("/tmp", "bad\0name"),
            ("/tmp", "bad\u{7f}name"),
        ] {
            assert!(
                validate_child(parent, name).is_err(),
                "{parent:?} / {name:?}"
            );
        }
        assert!(validate_child("/", "New project 中文").is_ok());
    }
}
