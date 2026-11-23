use crate::{
    error::Result,
    state::{AppState, Event},
};
use remote_codex_client::application::{
    UPDATE_CHECK_INTERVAL, UpdateComponent, UpdateMode, UpdateProgress, UpdateSnapshot, Updater,
};
use tauri::{AppHandle, Manager, State, ipc::Channel};

pub(super) fn updater() -> Result<Updater> {
    Ok(Updater::open(None, UpdateComponent::App)?)
}

pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Ok(updater) = updater() {
            #[cfg(windows)]
            let _ = &updater;
            loop {
                #[cfg(not(windows))]
                let result = updater.automatic_check().await;
                #[cfg(windows)]
                let result = super::windows_updates::automatic(&app).await;
                if let Err(error) = result {
                    eprintln!("Remote Codex update: {error:?}");
                }
                app.state::<AppState>().broadcast(Event::UpdatesChanged);
                tokio::time::sleep(UPDATE_CHECK_INTERVAL).await;
            }
        }
    });
}

#[tauri::command]
pub(crate) async fn update_status(app: AppHandle) -> Result<UpdateSnapshot> {
    snapshot(&app).await
}

#[tauri::command]
pub(crate) async fn update_check(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<UpdateSnapshot> {
    let result = updater()?.check().await;
    state.broadcast(Event::UpdatesChanged);
    result?;
    snapshot(&app).await
}

#[tauri::command]
pub(crate) async fn update_install(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: Channel<UpdateProgress>,
) -> Result<UpdateSnapshot> {
    #[cfg(not(windows))]
    let result = updater()?
        .install(&|progress| {
            let _ = channel.send(progress);
        })
        .await;
    #[cfg(windows)]
    let result = super::windows_updates::download(&app, |progress| {
        let _ = channel.send(progress);
    })
    .await;
    state.broadcast(Event::UpdatesChanged);
    result?;
    snapshot(&app).await
}

#[tauri::command]
pub(crate) async fn update_configure(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: UpdateMode,
) -> Result<UpdateSnapshot> {
    let result = updater()?.configure(mode).await;
    state.broadcast(Event::UpdatesChanged);
    result?;
    snapshot(&app).await
}

pub(super) async fn snapshot(app: &AppHandle) -> Result<UpdateSnapshot> {
    #[cfg(windows)]
    {
        super::windows_updates::snapshot(app).await
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Ok(updater()?.snapshot().await?)
    }
}
