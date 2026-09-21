use super::SessionHandle;
use crate::ClientError;
use remote_codex_adapter::gateway::Backend;
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, watch};

/// The terminal may switch threads; each session owner keeps its original binding.
pub(super) struct TerminalSessions {
    pub current: watch::Sender<SessionHandle>,
    previous: Mutex<Option<SessionHandle>>,
    creation: tokio::sync::Mutex<()>,
    events: broadcast::Sender<Value>,
    revoked: watch::Sender<bool>,
    closed: watch::Sender<Option<Option<String>>>,
    relay: tokio::task::JoinHandle<()>,
}

impl TerminalSessions {
    pub(super) fn new(initial: SessionHandle) -> Arc<Self> {
        let current = watch::channel(initial).0;
        let events = broadcast::channel(256).0;
        let revoked = watch::channel(false).0;
        let closed = watch::channel(None).0;
        let relay = tokio::spawn(forward(
            current.subscribe(),
            events.clone(),
            revoked.clone(),
            closed.clone(),
        ));
        Arc::new(Self {
            current,
            previous: Mutex::new(None),
            creation: tokio::sync::Mutex::new(()),
            events,
            revoked,
            closed,
            relay,
        })
    }

    async fn create(&self, method: &str, params: &Value) -> Result<Value, Fault> {
        let _creation = self.creation.lock().await;
        if self.closed.borrow().is_some() || *self.revoked.borrow() {
            return Err(Fault::new(
                "SESSION_CLOSED",
                "terminal control is no longer available",
            ));
        }
        if self.previous.lock().map_err(|_| state_error())?.is_some() {
            return Err(Fault::new(
                "SESSION_SWITCH_PENDING",
                "finish attaching to the current thread before creating another",
            ));
        }
        let source = self.current.borrow().clone();
        source
            .client
            .0
            .ensure_open()
            .map_err(ClientError::into_fault)?;
        let (runtime, response) = source
            .runtime
            .create_from_frontend(method, params)
            .await
            .map_err(ClientError::into_fault)?;
        if let Err(error) = source.client.0.register(&runtime) {
            runtime.shutdown().await;
            return Err(error.into_fault());
        }
        let next = SessionHandle::new(runtime, source.program.clone(), source.client.clone());
        *self.previous.lock().map_err(|_| state_error())? = Some(source);
        self.current.send_replace(next);
        Ok(response)
    }
}

impl Backend for TerminalSessions {
    async fn request(&self, method: &str, params: Value) -> Result<Value, Fault> {
        if matches!(method, "thread/start" | "thread/fork") {
            return self.create(method, &params).await;
        }
        let current = self.current.borrow().clone();
        if let Some(id) = params["threadId"].as_str()
            && id != current.session().id
        {
            let previous = {
                let mut slot = self.previous.lock().map_err(|_| state_error())?;
                if slot.as_ref().is_none_or(|s| s.session().id != id) {
                    return Err(Fault::new(
                        "SESSION_MISMATCH",
                        "thread does not belong to this terminal",
                    ));
                }
                if method == "thread/unsubscribe" {
                    slot.take()
                } else {
                    slot.clone()
                }
            }
            .ok_or_else(state_error)?;
            if method == "thread/unsubscribe" {
                previous.close().await;
                return Ok(json!({"status":"unsubscribed"}));
            }
            if !matches!(
                method,
                "thread/read" | "thread/turns/list" | "thread/items/list"
            ) {
                return Err(Fault::new(
                    "SESSION_MISMATCH",
                    "only the current terminal thread accepts new actions",
                ));
            }
            return previous.runtime.request(method, params).await;
        }
        current.runtime.request(method, params).await
    }

    fn respond(&self, response: Value) -> Result<(), Fault> {
        self.current.borrow().runtime.respond(response)
    }
    fn events(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
    fn pending_requests(&self) -> Result<Vec<Value>, Fault> {
        self.current.borrow().runtime.pending_requests()
    }
    fn revoked(&self) -> watch::Receiver<bool> {
        self.revoked.subscribe()
    }
    fn closed(&self) -> watch::Receiver<Option<Option<String>>> {
        self.closed.subscribe()
    }
    async fn close(&self) {
        self.closed.send_replace(Some(None));
        let _creation = self.creation.lock().await;
        let current = self.current.borrow().clone();
        let previous = self.previous.lock().ok().and_then(|mut slot| slot.take());
        current.close().await;
        if let Some(previous) = previous {
            previous.close().await;
        }
    }
}

async fn forward(
    mut changes: watch::Receiver<SessionHandle>,
    terminal_events: broadcast::Sender<Value>,
    terminal_revoked: watch::Sender<bool>,
    terminal_closed: watch::Sender<Option<Option<String>>>,
) {
    loop {
        let current = changes.borrow_and_update().clone();
        let mut events = Backend::events(&*current.runtime);
        let mut revoked = current.runtime.revoked();
        let mut closed = current.runtime.closed();
        if *revoked.borrow() || closed.borrow().is_some() {
            if changes.borrow().session().id != current.session().id {
                continue;
            }
            terminal_revoked.send_replace(*revoked.borrow());
            terminal_closed.send_replace(closed.borrow().clone());
            return;
        }
        loop {
            tokio::select! {
                biased;
                changed = changes.changed() => {
                    if changed.is_err() { return; }
                    break;
                }
                _ = revoked.changed() => {
                    if changes.borrow().session().id == current.session().id {
                        terminal_revoked.send_replace(true);
                        return;
                    }
                }
                _ = closed.changed() => {
                    if changes.borrow().session().id == current.session().id {
                        terminal_closed.send_replace(closed.borrow().clone());
                        return;
                    }
                }
                event = events.recv() => {
                    if changes.borrow().session().id != current.session().id { continue; }
                    match event {
                        Ok(event) => { let _ = terminal_events.send(event); }
                        Err(error) => {
                            terminal_closed.send_replace(Some(Some(format!("native terminal event stream failed: {error}"))));
                            return;
                        }
                    }
                }
            }
        }
    }
}

impl Drop for TerminalSessions {
    fn drop(&mut self) {
        self.relay.abort();
    }
}

fn state_error() -> Fault {
    Fault::new("SESSION_STATE", "terminal session state unavailable")
}
