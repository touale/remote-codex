use crate::{
    error::Result,
    state::{AppState, operation},
};
use remote_codex_client::application::ServerAuthentication;
use serde::Deserialize;
use tauri::{State, WebviewWindow};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(crate) enum Change {
    Password { value: String },
    ForgetPassword,
    Identity { path: Option<String> },
}
#[tauri::command]
pub(crate) async fn server_authentication(
    window: WebviewWindow,
    state: State<'_, AppState>,
    operation_id: String,
    name: String,
    change: Option<Change>,
) -> Result<ServerAuthentication> {
    let context = state.window(&window)?;
    operation(&context, operation_id, async {
        let servers = context.client.servers();
        match change {
            Some(Change::Password { value }) => {
                servers
                    .save_password(&name, zeroize::Zeroizing::new(value))
                    .await?
            }
            Some(Change::ForgetPassword) => servers.forget_password(&name).await?,
            Some(Change::Identity { path }) => {
                servers.save_identity(&name, path.map(Into::into)).await?
            }
            None => {}
        }
        Ok(servers.authentication(&name).await?)
    })
    .await
}
