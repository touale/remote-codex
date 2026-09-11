use crate::{
    error::{Error, Result},
    state::{AppState, unavailable},
    transfer_grants::Granted,
};
use remote_codex_client::application::Transfer;
use tauri::{State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub(crate) async fn transfer_pick(
    window: WebviewWindow,
    state: State<'_, AppState>,
    folder: bool,
) -> Result<Option<Granted>> {
    let context = state.window(&window)?;
    let (send, receive) = tokio::sync::oneshot::channel();
    let dialog = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_title(if folder {
            "Upload folder"
        } else {
            "Upload files"
        });
    if folder {
        dialog.pick_folder(move |path| {
            let _ = send.send(path.map(|p| vec![p]));
        });
    } else {
        dialog.pick_files(move |paths| {
            let _ = send.send(paths);
        });
    }
    let Some(paths) = receive.await.map_err(|_| unavailable())? else {
        return Ok(None);
    };
    let paths = paths
        .into_iter()
        .map(|p| {
            p.into_path()
                .map_err(|_| Error::new("TRANSFER_PATH", "Select local files."))
        })
        .collect::<Result<_>>()?;
    Ok(Some(context.grants.insert(paths)?))
}
#[tauri::command]
pub(crate) async fn transfer_upload(
    window: WebviewWindow,
    state: State<'_, AppState>,
    context: String,
    destination: String,
    token: String,
) -> Result<Transfer> {
    let window_state = state.window(&window)?;
    let files = window_state.files(&context)?;
    let paths = window_state.grants.take(&token)?;
    Ok(window_state
        .client
        .transfers()
        .upload(files.server(), files.path(), &destination, paths)
        .await?)
}
#[tauri::command]
pub(crate) async fn transfer_download(
    window: WebviewWindow,
    state: State<'_, AppState>,
    context: String,
    source: String,
) -> Result<Option<Transfer>> {
    let window_state = state.window(&window)?;
    let files = window_state.files(&context)?;
    let (send, receive) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .file()
        .set_parent(&window)
        .set_title("Download to folder")
        .pick_folder(move |path| {
            let _ = send.send(path);
        });
    let Some(path) = receive.await.map_err(|_| unavailable())? else {
        return Ok(None);
    };
    let path = path
        .into_path()
        .map_err(|_| Error::new("TRANSFER_PATH", "Select a local folder."))?;
    Ok(Some(
        window_state
            .client
            .transfers()
            .download(files.server(), files.path(), &source, path)
            .await?,
    ))
}
#[tauri::command]
pub(crate) async fn transfer_discard_grant(
    window: WebviewWindow,
    state: State<'_, AppState>,
    token: String,
) -> Result<()> {
    let _ = state.window(&window)?.grants.take(&token);
    Ok(())
}
