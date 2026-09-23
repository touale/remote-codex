use crate::{
    error::{Error, Result},
    state::{AppState, Event, unavailable},
};
use std::sync::atomic::Ordering;
use tauri::{Manager, State, WebviewWindow};

use super::editor_windows as content;
mod layout;
mod target;
pub(crate) use content::FileTransfer;
pub(crate) use layout::{Preferences, load_preferences, preferences_root};
use layout::{preferences_path, validate};
pub(crate) use target::WindowTarget;

#[tauri::command]
pub(crate) async fn preferences(
    window: WebviewWindow,
    state: State<'_, AppState>,
    value: Option<Preferences>,
) -> Result<Preferences> {
    state.window(&window)?;
    if let Some(value) = value {
        validate(&value)?;
        let path = preferences_path(&window)?;
        let stage = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&stage, serde_json::to_vec(&value)?)?;
        std::fs::rename(stage, path)?;
        let title = state
            .startup_targets
            .lock()
            .map_err(|_| unavailable())?
            .get(window.label())
            .and_then(WindowTarget::content_title);
        window.set_title(&title.unwrap_or_else(|| {
            value
                .selected_workspace
                .as_ref()
                .map(|(server, path)| format!("{server} · {path} — Remote Codex"))
                .unwrap_or_else(|| "Remote Codex".into())
        }))?;
        Ok(value)
    } else {
        load_preferences(&window)
    }
}
#[tauri::command]
pub(crate) async fn authentication_answer(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    answer: Option<String>,
) -> Result<()> {
    let context = state.window(&window)?;
    let sender = context
        .authentication
        .lock()
        .map_err(|_| unavailable())?
        .remove(&id)
        .ok_or_else(unavailable)?;
    let _ = sender.send(answer.map(zeroize::Zeroizing::new));
    Ok(())
}
#[tauri::command]
pub(crate) async fn acknowledge(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    if let Some(sender) = state
        .window(&window)?
        .deliveries
        .lock()
        .map_err(|_| unavailable())?
        .remove(&id)
    {
        let _ = sender.send(());
    }
    Ok(())
}
#[tauri::command]
pub(crate) async fn cancel_operation(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> Result<()> {
    let context = state.window(&window)?;
    if let Some(operation) = context
        .operations
        .lock()
        .map_err(|_| unavailable())?
        .remove(&id)
    {
        operation.abort();
    }
    Ok(())
}
#[tauri::command]
pub(crate) async fn new_window(
    window: WebviewWindow,
    state: State<'_, AppState>,
    target: Option<WindowTarget>,
) -> Result<String> {
    let gate = state.initialization.lock().await;
    if state.restarting.load(Ordering::Acquire) {
        return Err(Error::new("APP_RESTARTING", "The app is restarting."));
    }
    let context = state.window(&window)?;
    let mut preferences = load_preferences(&window)?;
    preferences.selected_workspace = match &target {
        Some(target) => target.validate(&context, &state).await?,
        None => None,
    };
    let label = format!("workspace-{}", uuid::Uuid::new_v4());
    let path = preferences_path(&window)?.with_file_name(format!("window-{label}.json"));
    let content_window = target.as_ref().is_some_and(WindowTarget::is_content);
    std::fs::write(&path, serde_json::to_vec(&preferences)?)?;
    if let Some(target) = &target {
        state
            .startup_targets
            .lock()
            .map_err(|_| unavailable())?
            .insert(label.clone(), target.clone());
    }
    let receiver = if let Some(WindowTarget::File { transfer, .. }) = &target {
        let (complete, receiver) = tokio::sync::oneshot::channel();
        state
            .file_transfers
            .lock()
            .map_err(|_| unavailable())?
            .insert(
                label.clone(),
                FileTransfer {
                    owner: window.label().into(),
                    id: transfer.clone(),
                    complete,
                },
            );
        Some(receiver)
    } else {
        None
    };
    let builder = tauri::WebviewWindowBuilder::new(
        window.app_handle(),
        label.clone(),
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title(
        target
            .as_ref()
            .and_then(WindowTarget::content_title)
            .unwrap_or_else(|| {
                preferences
                    .selected_workspace
                    .as_ref()
                    .map(|(server, path)| format!("{server} · {path} — Remote Codex"))
                    .unwrap_or_else(|| match &target {
                        Some(WindowTarget::Server { server }) => format!("{server} — Remote Codex"),
                        _ => "Remote Codex".into(),
                    })
            }),
    )
    .inner_size(
        if content_window { 1040.0 } else { 1440.0 },
        if content_window { 760.0 } else { 900.0 },
    )
    .min_inner_size(
        if content_window { 640.0 } else { 900.0 },
        if content_window { 480.0 } else { 600.0 },
    )
    .title_bar_style(tauri::TitleBarStyle::Overlay)
    .hidden_title(true);
    #[cfg(feature = "e2e")]
    let builder =
        builder.background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    let built = builder.build();
    if let Err(error) = built {
        state
            .file_transfers
            .lock()
            .map_err(|_| unavailable())?
            .remove(&label);
        state
            .startup_targets
            .lock()
            .map_err(|_| unavailable())?
            .remove(&label);
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    drop(gate);
    state.changed();
    if let Some(receiver) = receiver
        && let Err(error) = receiver.await.unwrap_or_else(|_| Err(unavailable()))
    {
        content::discard(&window, &state, &label).await;
        return Err(error);
    }
    Ok(label)
}
#[tauri::command]
pub(crate) async fn close_window(
    window: WebviewWindow,
    state: State<'_, AppState>,
    cancel: bool,
) -> Result<()> {
    if cancel {
        state.restarting.store(false, Ordering::Release);
        return Ok(());
    }
    let context = state.window(&window)?;
    content::cancel_for(&state, window.label())?;
    state
        .startup_targets
        .lock()
        .map_err(|_| unavailable())?
        .remove(window.label());
    context.close().await;
    // Only the window that removes the last entry may exit or restart the app.
    let last_window = {
        let mut windows = state.windows.lock().map_err(|_| unavailable())?;
        windows.remove(window.label()).is_some() && windows.is_empty()
    };
    window.destroy()?;
    state.changed();
    if last_window {
        if state.restarting.swap(false, Ordering::AcqRel) {
            window.app_handle().request_restart();
        } else {
            window.app_handle().exit(0);
        }
    }
    Ok(())
}
#[tauri::command]
pub(crate) async fn external_link(
    window: WebviewWindow,
    state: State<'_, AppState>,
    url: String,
) -> Result<()> {
    state.window(&window)?;
    let parsed =
        url::Url::parse(&url).map_err(|_| Error::new("INVALID_LINK", "Invalid web link."))?;
    if !matches!(parsed.scheme(), "https" | "http")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(Error::new("INVALID_LINK", "Only web links can be opened."));
    }
    let status = tokio::process::Command::new("/usr/bin/open")
        .arg("--")
        .arg(parsed.as_str())
        .status()
        .await?;
    if !status.success() {
        return Err(Error::new(
            "LINK_FAILED",
            "Cannot open this link in the browser.",
        ));
    }
    Ok(())
}
pub(crate) fn request_close(window: &WebviewWindow) -> bool {
    let state = window.state::<AppState>();
    if let Ok(context) = state.window(window)
        && !context.closing.load(Ordering::Acquire)
    {
        context.send(Event::CloseRequested);
        return true;
    }
    false
}
pub(crate) fn destroyed(app: &tauri::AppHandle, label: &str) {
    let state = app.state::<AppState>();
    let _ = content::cancel_for(&state, label);
    if let Ok(mut targets) = state.startup_targets.lock() {
        targets.remove(label);
    }
    if let Ok(mut windows) = state.windows.lock()
        && let Some(context) = windows.remove(label)
    {
        tauri::async_runtime::spawn(async move {
            context.close().await;
        });
    }
}
