use crate::{ClientError, Result};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Notify;
#[derive(Default)]
pub(crate) struct Runtime {
    active: Mutex<HashMap<String, Arc<Control>>>,
    changed: Notify,
    closed: AtomicBool,
}
#[derive(Default)]
pub(crate) struct Control {
    stopped: AtomicBool,
    stop: Notify,
}
pub(crate) struct Registration {
    runtime: Arc<Runtime>,
    id: String,
    pub control: Arc<Control>,
}
impl Runtime {
    pub(crate) fn register(self: &Arc<Self>, id: &str) -> Result<Registration> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(ClientError::Argument("Application client is closed."));
        }
        if active.contains_key(id) {
            return Err(ClientError::Argument("Transfer is already running."));
        }
        let control = Arc::new(Control::default());
        active.insert(id.into(), control.clone());
        Ok(Registration {
            runtime: self.clone(),
            id: id.into(),
            control,
        })
    }
    pub(crate) fn owns(&self, id: &str) -> bool {
        self.active.lock().is_ok_and(|a| a.contains_key(id))
    }
    pub(crate) fn running_ids(&self) -> Result<Vec<String>> {
        self.active
            .lock()
            .map(|active| active.keys().cloned().collect())
            .map_err(|_| ClientError::RemoteResponse)
    }
    pub(crate) async fn pause(&self, id: &str) {
        loop {
            let changed = self.changed.notified();
            let control = self.active.lock().ok().and_then(|a| a.get(id).cloned());
            let Some(control) = control else {
                break;
            };
            control.stopped.store(true, Ordering::Release);
            control.stop.notify_one();
            changed.await;
        }
    }
    pub(crate) async fn close(&self) {
        self.closed.store(true, Ordering::Release);
        let ids = self
            .active
            .lock()
            .map(|a| a.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for id in ids {
            self.pause(&id).await;
        }
    }
}
impl Control {
    pub(crate) async fn cancelled(&self) {
        if !self.stopped.load(Ordering::Acquire) {
            self.stop.notified().await;
        }
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        if let Ok(mut active) = self.runtime.active.lock() {
            active.remove(&self.id);
        }
        self.runtime.changed.notify_waiters();
    }
}

static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
pub(crate) async fn slot() -> Result<tokio::sync::SemaphorePermit<'static>> {
    SLOTS
        .acquire()
        .await
        .map_err(|_| ClientError::Argument("Transfer scheduler is closed."))
}
