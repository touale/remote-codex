use crate::{
    error::{Error, Result},
    state::{AppState, Event, unavailable},
};
use std::sync::atomic::Ordering;
use tauri::{Manager, State, WebviewWindow};

mod layout;
mod target;
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
        window.set_title(
            &value
                .selected_workspace
                .as_ref()
                .map(|(server, path)| format!("{server} · {path} — Remote Codex"))
                .unwrap_or_else(|| "Remote Codex".into()),
        )?;
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
    let _gate = state.initialization.lock().await;
    let context = state.window(&window)?;
    let mut preferences = load_preferences(&window)?;
    preferences.selected_workspace = match &target {
        Some(target) => target.validate(&context, &state).await?,
        None => None,
    };
    let label = format!("workspace-{}", uuid::Uuid::new_v4());
    let path = preferences_path(&window)?.with_file_name(format!("window-{label}.json"));
    std::fs::write(&path, serde_json::to_vec(&preferences)?)?;
    if let Some(target) = &target {
        state
            .startup_targets
            .lock()
            .map_err(|_| unavailable())?
            .insert(label.clone(), target.clone());
    }
    let builder = tauri::WebviewWindowBuilder::new(
        window.app_handle(),
        label.clone(),
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title(
        preferences
            .selected_workspace
            .as_ref()
            .map(|(server, path)| format!("{server} · {path} — Remote Codex"))
            .unwrap_or_else(|| match &target {
                Some(WindowTarget::Server { server }) => format!("{server} — Remote Codex"),
                _ => "Remote Codex".into(),
            }),
    )
    .inner_size(1440.0, 900.0)
    .min_inner_size(900.0, 600.0)
    .title_bar_style(tauri::TitleBarStyle::Overlay)
    .hidden_title(true);
    #[cfg(feature = "e2e")]
    let builder =
        builder.background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    let built = builder.build();
    if let Err(error) = built {
        state
            .startup_targets
            .lock()
            .map_err(|_| unavailable())?
            .remove(&label);
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    state.changed();
    Ok(label)
}
#[tauri::command]
pub(crate) async fn close_window(
    window: WebviewWindow,
    state: State<'_, AppState>,
    cancel: bool,
) -> Result<()> {
    if cancel {
        state.quitting.store(false, Ordering::Release);
        return Ok(());
    }
    let context = state.window(&window)?;
    context.close().await;
    state
        .windows
        .lock()
        .map_err(|_| unavailable())?
        .remove(window.label());
    window.destroy()?;
    state.changed();
    if state.windows.lock().map_err(|_| unavailable())?.is_empty() {
        window.app_handle().exit(0);
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
