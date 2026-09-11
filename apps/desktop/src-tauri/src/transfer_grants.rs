use crate::{
    error::{Error, Result},
    state::{AppState, Event, unavailable},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::Manager;
#[derive(Default)]
pub(crate) struct Grants(Mutex<HashMap<String, (Instant, Vec<PathBuf>)>>);
#[derive(serde::Serialize)]
pub(crate) struct Granted {
    pub token: String,
    pub names: Vec<String>,
}
impl Grants {
    pub(crate) fn insert(&self, paths: Vec<PathBuf>) -> Result<Granted> {
        if paths.is_empty() || paths.len() > 1000 {
            return Err(Error::new(
                "TRANSFER_SELECTION",
                "Select between 1 and 1,000 entries.",
            ));
        }
        let names = paths
            .iter()
            .map(|p| {
                p.file_name()
                    .and_then(|p| p.to_str())
                    .map(str::to_owned)
                    .ok_or_else(|| Error::new("TRANSFER_NAME", "This file name is not supported."))
            })
            .collect::<Result<_>>()?;
        let mut grants = self.0.lock().map_err(|_| unavailable())?;
        grants.retain(|_, (created, _)| created.elapsed() < Duration::from_secs(600));
        if grants.len() >= 8 {
            grants.clear();
        }
        let token = uuid::Uuid::new_v4().to_string();
        grants.insert(token.clone(), (Instant::now(), paths));
        Ok(Granted { token, names })
    }
    pub(crate) fn take(&self, token: &str) -> Result<Vec<PathBuf>> {
        let (created, paths) = self
            .0
            .lock()
            .map_err(|_| unavailable())?
            .remove(token)
            .ok_or_else(unavailable)?;
        if created.elapsed() >= Duration::from_secs(600) {
            return Err(Error::new(
                "TRANSFER_SELECTION_EXPIRED",
                "Select the files again.",
            ));
        }
        Ok(paths)
    }
}
pub(crate) fn dropped(webview: &tauri::Webview, event: &tauri::WebviewEvent) {
    let tauri::WebviewEvent::DragDrop(tauri::DragDropEvent::Drop { paths, position }) = event
    else {
        return;
    };
    let state = webview.app_handle().state::<AppState>();
    let context = state
        .windows
        .lock()
        .ok()
        .and_then(|w| w.get(webview.label()).cloned());
    if let Some(context) = context {
        match context.grants.insert(paths.clone()) {
            Ok(grant) => context.send(Event::FilesDropped {
                token: grant.token,
                names: grant.names,
                x: position.x,
                y: position.y,
            }),
            Err(_) => context.send(Event::Notice {
                message: "These files could not be selected for upload.".into(),
            }),
        }
    }
}
