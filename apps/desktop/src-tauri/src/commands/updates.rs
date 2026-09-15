use crate::error::Result;
use remote_codex_client::application::{
    UPDATE_CHECK_INTERVAL, UpdateComponent, UpdateMode, UpdateProgress, UpdateSnapshot, Updater,
};
use tauri::ipc::Channel;

fn updater() -> Result<Updater> {
    Ok(Updater::open(None, UpdateComponent::App)?)
}

pub(crate) fn start() {
    tauri::async_runtime::spawn(async {
        if let Ok(updater) = updater() {
            loop {
                if let Err(error) = updater.automatic_check().await {
                    eprintln!("Remote Codex update: {error}");
                }
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
pub(crate) async fn update_check() -> Result<UpdateSnapshot> {
    Ok(updater()?.check().await?)
}

#[tauri::command]
pub(crate) async fn update_install(channel: Channel<UpdateProgress>) -> Result<UpdateSnapshot> {
    Ok(updater()?
        .install(&|progress| {
            let _ = channel.send(progress);
        })
        .await?)
}

#[tauri::command]
pub(crate) async fn update_configure(mode: UpdateMode) -> Result<UpdateSnapshot> {
    Ok(updater()?.configure(mode).await?)
}
