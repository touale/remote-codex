use crate::{
    error::{Error, Result},
    state::{AppState, WindowState},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum WindowTarget {
    Server { server: String },
    Workspace { server: String, path: String },
    Session { id: String },
}
impl WindowTarget {
    pub(crate) async fn validate(
        &self,
        context: &WindowState,
        state: &AppState,
    ) -> Result<Option<(String, String)>> {
        match self {
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
