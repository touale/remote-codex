use crate::{
    error::Result,
    state::{AppState, Event},
};
use remote_codex_client::application::{
    UPDATE_CHECK_INTERVAL, UpdateComponent, UpdateMode, UpdateProgress, UpdateSnapshot, Updater,
};
use tauri::{AppHandle, Manager, State, ipc::Channel};

fn updater() -> Result<Updater> {
    Ok(Updater::open(None, UpdateComponent::App)?)
}

pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Ok(updater) = updater() {
            loop {
                if let Err(error) = updater.automatic_check().await {
                    eprintln!("Remote Codex update: {error}");
                }
                app.state::<AppState>().broadcast(Event::UpdatesChanged);
                tokio::time::sleep(UPDATE_CHECK_INTERVAL).await;
            }
        }
    });
}

#[tauri::command]
pub(crate) async fn update_status() -> Result<UpdateSnapshot> {
    Ok(updater()?.snapshot().await?)
}

#[tauri::command]
pub(crate) async fn update_check(state: State<'_, AppState>) -> Result<UpdateSnapshot> {
    let result = updater()?.check().await;
    state.broadcast(Event::UpdatesChanged);
    Ok(result?)
}

#[tauri::command]
pub(crate) async fn update_install(
    state: State<'_, AppState>,
    channel: Channel<UpdateProgress>,
) -> Result<UpdateSnapshot> {
    let result = updater()?
        .install(&|progress| {
            let _ = channel.send(progress);
        })
        .await;
    state.broadcast(Event::UpdatesChanged);
    Ok(result?)
}

#[tauri::command]
pub(crate) async fn update_configure(
    state: State<'_, AppState>,
    mode: UpdateMode,
) -> Result<UpdateSnapshot> {
    let result = updater()?.configure(mode).await;
    state.broadcast(Event::UpdatesChanged);
    Ok(result?)
}
