use crate::error::{Error, Result};
use remote_codex_client::application::*;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Manager, WebviewWindow, ipc::Channel};
use tokio::sync::oneshot;
use zeroize::Zeroizing;

include!(concat!(env!("OUT_DIR"), "/bundled.rs"));

pub(crate) use crate::events::Event;

#[derive(Default)]
pub(crate) struct AppState {
    pub windows: Mutex<HashMap<String, Arc<WindowState>>>,
    pub preferences: Mutex<Option<crate::commands::app_preferences::AppPreferences>>,
    pub startup_workspaces: Mutex<HashMap<String, (String, String)>>,
    pub quitting: AtomicBool,
    pub openings: crate::opening::Openings,
    pub initialization: tokio::sync::Mutex<()>,
}
pub(crate) struct WindowState {
    pub client: Client,
    pub grants: crate::transfer_grants::Grants,
    pub sessions: Mutex<HashMap<String, SessionHandle>>,
    pub workspaces: Mutex<HashMap<String, WorkspaceHandle>>,
    pub file_contexts: Mutex<HashMap<String, FileHandle>>,
    pub browsers: Mutex<HashMap<String, DirectoryBrowser>>,
    pub terminals: Mutex<HashMap<String, Arc<ShellHandle>>>,
    pub prepared: Mutex<HashMap<String, crate::opening::PreparedOpen>>,
    pub native: tokio::sync::Mutex<Option<NativeHandle>>,
    pub events: Arc<Mutex<Channel<Event>>>,
    pub authentication: crate::authentication::Replies,
    pub operations: Mutex<HashMap<String, futures_util::future::AbortHandle>>,
    pub closing: AtomicBool,
    pub deliveries: Mutex<HashMap<String, oneshot::Sender<()>>>,
    pub streams: Mutex<HashMap<String, tokio::task::AbortHandle>>,
}

impl AppState {
    pub(crate) fn window(&self, window: &WebviewWindow) -> Result<Arc<WindowState>> {
        self.windows
            .lock()
            .map_err(|_| unavailable())?
            .get(window.label())
            .cloned()
            .ok_or_else(unavailable)
    }
    pub(crate) fn changed(&self) {
        self.broadcast(Event::CatalogChanged);
    }
    pub(crate) fn broadcast(&self, event: Event) {
        if let Ok(windows) = self.windows.lock() {
            for window in windows.values() {
                window.send(event.clone());
            }
        }
    }
}
impl WindowState {
    pub(crate) async fn new(
        channel: Channel<Event>,
        program: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        let events = Arc::new(Mutex::new(channel));
        let authentication = Arc::new(Mutex::new(HashMap::<
            String,
            oneshot::Sender<Option<Zeroizing<String>>>,
        >::new()));
        let notice = events.clone();
        let client = Client::open(ClientOptions {
            desktop_discovery: true, codex_program: program,
            service: ServiceBundle { bytes: Arc::from(SERVICE), sha256: SERVICE_SHA.into() },
            authentication: Some(crate::authentication::handler(authentication.clone(), events.clone())),
            notice: Some(Arc::new(move |_| { if let Ok(channel) = notice.lock() { let _ = channel.send(Event::Notice { message: "Some retired credentials could not be removed. Cleanup will retry later.".into() }); } })),
            ..Default::default()
        }).await?;
        Ok(Self {
            client,
            grants: Default::default(),
            events,
            authentication,
            sessions: Mutex::new(HashMap::new()),
            workspaces: Mutex::new(HashMap::new()),
            file_contexts: Mutex::new(HashMap::new()),
            browsers: Mutex::new(HashMap::new()),
            terminals: Mutex::new(HashMap::new()),
            prepared: Mutex::new(HashMap::new()),
            native: tokio::sync::Mutex::new(None),
            operations: Mutex::new(HashMap::new()),
            closing: AtomicBool::new(false),
            deliveries: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
        })
    }
    pub(crate) fn send(&self, event: Event) {
        if let Ok(channel) = self.events.lock() {
            let _ = channel.send(event);
        }
    }
    pub(crate) fn session(&self, id: &str) -> Result<SessionHandle> {
        self.sessions
            .lock()
            .map_err(|_| unavailable())?
            .get(id)
            .cloned()
            .ok_or_else(unavailable)
    }
    pub(crate) fn workspace(&self, id: &str) -> Result<WorkspaceHandle> {
        self.workspaces
            .lock()
            .map_err(|_| unavailable())?
            .get(id)
            .cloned()
            .ok_or_else(unavailable)
    }
    pub(crate) fn files(&self, id: &str) -> Result<FileHandle> {
        if let Some(workspace) = self.workspaces.lock().map_err(|_| unavailable())?.get(id) {
            return Ok(workspace.files().clone());
        }
        self.file_contexts
            .lock()
            .map_err(|_| unavailable())?
            .get(id)
            .cloned()
            .ok_or_else(unavailable)
    }
    pub(crate) fn terminal(&self, id: &str) -> Result<Arc<ShellHandle>> {
        self.terminals
            .lock()
            .map_err(|_| unavailable())?
            .get(id)
            .cloned()
            .ok_or_else(unavailable)
    }
    pub(crate) async fn close_files(&self, server: Option<&str>, path: Option<&str>) -> Result<()> {
        let mut closing = Vec::new();
        let matches = |files: &FileHandle| {
            server.is_none_or(|s| files.server() == s) && path.is_none_or(|p| files.path() == p)
        };
        {
            let mut workspaces = self.workspaces.lock().map_err(|_| unavailable())?;
            workspaces.retain(|_, workspace| {
                if matches(workspace.files()) {
                    closing.push(workspace.files().clone());
                    false
                } else {
                    true
                }
            });
        }
        {
            let mut contexts = self.file_contexts.lock().map_err(|_| unavailable())?;
            contexts.retain(|_, files| {
                if matches(files) {
                    closing.push(files.clone());
                    false
                } else {
                    true
                }
            });
        }
        for files in &closing {
            files.close();
        }
        for files in closing {
            files.shutdown().await;
        }
        Ok(())
    }
    pub(crate) async fn close(&self) {
        if self.closing.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Ok(mut streams) = self.streams.lock() {
            for (_, stream) in streams.drain() {
                stream.abort();
            }
        }
        if let Ok(mut pending) = self.deliveries.lock() {
            pending.clear();
        }
        if let Ok(mut operations) = self.operations.lock() {
            for (_, operation) in operations.drain() {
                operation.abort();
            }
        }
        if let Ok(mut prompts) = self.authentication.lock() {
            prompts.clear();
        }
        if let Ok(mut prepared) = self.prepared.lock() {
            prepared.clear();
        }
        if let Ok(mut terminals) = self.terminals.lock() {
            for (_, terminal) in terminals.drain() {
                terminal.close();
            }
        }
        if let Ok(mut browsers) = self.browsers.lock() {
            for (_, browser) in browsers.drain() {
                browser.close();
            }
        }
        let _ = self.close_files(None, None).await;
        self.client.close().await;
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.clear();
        }
        if let Some(native) = self.native.lock().await.take() {
            native.close().await;
        }
    }
}

pub(crate) fn unavailable() -> Error {
    Error::new(
        "RESOURCE_UNAVAILABLE",
        "This resource is closed or belongs to another window.",
    )
}

pub(crate) async fn operation<T, F>(context: &Arc<WindowState>, id: String, future: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    if context.closing.load(Ordering::Acquire) {
        return Err(unavailable());
    }
    let (handle, registration) = futures_util::future::AbortHandle::new_pair();
    {
        let mut operations = context.operations.lock().map_err(|_| unavailable())?;
        if operations.len() >= 32 || operations.contains_key(&id) {
            return Err(Error::new("OPERATION_BUSY", "Too many pending operations."));
        }
        operations.insert(id.clone(), handle);
    }
    let output = context.events.clone();
    let operation = id.clone();
    let handler = Arc::new(move |event| {
        if let Ok(channel) = output.lock() {
            let _ = channel.send(Event::Progress {
                operation: operation.clone(),
                event,
            });
        }
    });
    let result = futures_util::future::Abortable::new(
        remote_codex_client::progress::with_progress(handler, future),
        registration,
    )
    .await;
    if let Ok(mut operations) = context.operations.lock() {
        operations.remove(&id);
    }
    context.send(Event::OperationFinished { id });
    result.map_err(|_| {
        Error::new(
            "OPERATION_CANCELLED",
            "Operation cancelled. Inspect any submitted work before retrying.",
        )
    })?
}

pub(crate) fn owned_elsewhere(
    app: &tauri::AppHandle,
    label: &str,
    session: &str,
) -> Result<Option<String>> {
    let state = app.state::<AppState>();
    let windows = state.windows.lock().map_err(|_| unavailable())?;
    for (owner, context) in windows.iter() {
        if owner != label
            && context
                .sessions
                .lock()
                .map_err(|_| unavailable())?
                .contains_key(session)
        {
            if let Some(window) = app.get_webview_window(owner) {
                window.unminimize()?;
                window.set_focus()?;
                context.send(Event::FocusSession { id: session.into() });
            }
            return Ok(Some(owner.clone()));
        }
    }
    Ok(None)
}
