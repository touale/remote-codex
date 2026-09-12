use super::content::{ChangeDocument, FileDocument, check_file};
use crate::{
    error::{Error, Result},
    state::{AppState, WindowState},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum WindowTarget {
    Server {
        server: String,
    },
    Workspace {
        server: String,
        path: String,
    },
    Session {
        id: String,
    },
    File {
        context: String,
        transfer: String,
        document: FileDocument,
    },
    Diff {
        document: ChangeDocument,
    },
}
impl WindowTarget {
    pub(crate) fn is_content(&self) -> bool {
        matches!(self, Self::File { .. } | Self::Diff { .. })
    }
    pub(crate) fn content_title(&self) -> Option<String> {
        match self {
            Self::File { document, .. } => Some(format!(
                "{} · {} — Remote Codex",
                document.path, document.server
            )),
            Self::Diff { document } => Some(format!(
                "Changes · {} · {} — Remote Codex",
                document.path, document.server
            )),
            _ => None,
        }
    }
    pub(crate) async fn validate(
        &self,
        context: &WindowState,
        state: &AppState,
    ) -> Result<Option<(String, String)>> {
        match self {
            Self::File {
                context: id,
                transfer,
                document,
            } => {
                uuid::Uuid::parse_str(transfer)
                    .map_err(|_| Error::new("INVALID_TRANSFER", "Invalid file transfer."))?;
                check_file(document)?;
                let files = context.files(id)?;
                if files.server() != document.server || files.path() != document.root {
                    return Err(Error::new(
                        "INVALID_FILE",
                        "This file belongs to a different location.",
                    ));
                }
                Ok(None)
            }
            Self::Diff { document } => {
                if document.diff.len() > 4 * 1024 * 1024 {
                    return Err(Error::new(
                        "FILE_TOO_LARGE",
                        "This patch is too large for a separate window.",
                    ));
                }
                Ok(None)
            }
            Self::Server { server } => {
                if !context
                    .client
                    .servers()
                    .saved()
                    .await?
                    .iter()
                    .any(|s| s.name == *server)
                {
                    return Err(Error::new(
                        "NOT_FOUND",
                        "This server is no longer available.",
                    ));
                }
                Ok(None)
            }
            Self::Workspace { server, path } => {
                if !context
                    .client
                    .workspaces()
                    .list()
                    .await?
                    .iter()
                    .any(|w| w.server == *server && w.path == *path)
                {
                    return Err(Error::new(
                        "NOT_FOUND",
                        "This workspace is no longer available.",
                    ));
                }
                Ok(Some((server.clone(), path.clone())))
            }
            Self::Session { id } => {
                if state.open_session_ids()?.contains(id) {
                    return Err(Error::new(
                        "SESSION_IN_USE",
                        "This session is already open. Close it before opening it in a new window.",
                    ));
                }
                let session = context.client.sessions().resolve(id, None).await?;
                Ok(Some((session.server, session.session.cwd)))
            }
        }
    }
}
