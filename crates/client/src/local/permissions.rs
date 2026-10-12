use remote_codex_adapter::thread::Codex;
use remote_codex_protocol::{Fault, SessionPermissions};
use serde_json::Value;
use std::{collections::HashMap, sync::Mutex};

#[cfg(test)]
#[path = "permissions_tests.rs"]
mod tests;

#[derive(Default)]
pub(crate) struct Permissions {
    state: Mutex<State>,
    changed: tokio::sync::Notify,
}

#[derive(Default)]
struct State {
    channel: String,
    thread: String,
    grant: Option<SessionPermissions>,
    pending: Option<Change>,
    native: Option<Codex>,
    children: HashMap<String, SessionPermissions>,
}

struct Change {
    id: String,
    full: bool,
    observed: bool,
    acknowledged: bool,
}

impl Permissions {
    pub(crate) fn restore(
        &self,
        channel: &str,
        thread: &str,
        full_access: bool,
    ) -> Result<(), Fault> {
        let mut state = self.state.lock().map_err(|_| invalid())?;
        *state = State::default();
        state.channel = channel.into();
        state.thread = thread.into();
        state.grant = full_access.then(|| state.grant());
        Ok(())
    }

    pub(crate) fn attach(&self, native: Codex) -> Result<(), Fault> {
        self.state.lock().map_err(|_| invalid())?.native = Some(native);
        Ok(())
    }

    /// Only ancestry returned by our native backend can expand this session's scope.
    pub(crate) async fn contains(&self, thread: &str) -> Result<bool, Fault> {
        let (native, channel, root) = {
            let state = self.state.lock().map_err(|_| invalid())?;
            if thread.is_empty() || state.thread.is_empty() {
                return Ok(false);
            }
            if thread == state.thread || state.children.contains_key(thread) {
                return Ok(true);
            }
            let Some(native) = state.native.clone() else {
                return Ok(false);
            };
            (native, state.channel.clone(), state.thread.clone())
        };
        let mut path = Vec::new();
        let mut current = thread.to_owned();
        loop {
            {
                let state = self.state.lock().map_err(|_| invalid())?;
                if state.channel != channel || state.thread != root {
                    return Ok(false);
                }
                if current == root || state.children.contains_key(&current) {
                    break;
                }
            }
            if path.contains(&current) {
                return Ok(false);
            }
            path.push(current.clone());
            let Some(parent) = native.parent_thread(&current).await? else {
                return Ok(false);
            };
            current = parent;
        }
        let mut state = self.state.lock().map_err(|_| invalid())?;
        if state.channel != channel || state.thread != root {
            return Ok(false);
        }
        for child in path {
            let grant = SessionPermissions {
                id: uuid::Uuid::new_v4().to_string(),
                channel: channel.clone(),
                thread: child.clone(),
            };
            state.children.entry(child).or_insert(grant);
        }
        Ok(true)
    }

    /// Record frontend intent. An expansion needs both a native response and
    /// notification; a reduction stops issuing full-access frames immediately.
    pub(crate) fn begin(
        &self,
        full: Option<bool>,
        supported: bool,
    ) -> Result<Option<String>, Fault> {
        let Some(full) = full else {
            return Ok(None);
        };
        if full && !supported {
            return Err(Fault::new(
                "SERVICE_UPDATE_REQUIRED",
                "remote service must be updated to support session Full Access",
            ));
        }
        let mut state = self.state.lock().map_err(|_| invalid())?;
        if state.pending.is_some() {
            return Err(Fault::new(
                "SETTINGS_BUSY",
                "previous permission change is still being applied",
            ));
        }
        let observed = state.grant.is_some() == full;
        if !full {
            state.grant = None;
        }
        let id = uuid::Uuid::new_v4().to_string();
        state.pending = Some(Change {
            id: id.clone(),
            full,
            observed,
            acknowledged: false,
        });
        Ok(Some(id))
    }

    pub(crate) fn acknowledge(&self, id: &str, success: bool) -> Result<(), Fault> {
        let mut state = self.state.lock().map_err(|_| invalid())?;
        if let Some(change) = &mut state.pending
            && change.id == id
        {
            if success {
                change.acknowledged = true;
            } else {
                state.pending = None;
            }
        }
        state.commit();
        self.changed.notify_waiters();
        Ok(())
    }

    pub(crate) fn observe(&self, thread: &str, full: bool) -> Result<(), Fault> {
        let mut state = self.state.lock().map_err(|_| invalid())?;
        if thread != state.thread {
            return Ok(());
        }
        if !full {
            state.grant = None;
        }
        if let Some(change) = &mut state.pending
            && change.full == full
        {
            change.observed = true;
        }
        state.commit();
        self.changed.notify_waiters();
        Ok(())
    }

    pub(crate) fn full_access(&self) -> bool {
        self.state.lock().is_ok_and(|state| state.grant.is_some())
    }

    pub(crate) async fn confirmed(&self, id: &str) -> Result<(), Fault> {
        let wait = async {
            loop {
                let changed = self.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if !self
                    .state
                    .lock()
                    .map_err(|_| invalid())?
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.id == id)
                {
                    return Ok(());
                }
                changed.await;
            }
        };
        match tokio::time::timeout(std::time::Duration::from_secs(3), wait).await {
            Ok(result) => result,
            Err(_) => {
                let mut state = self.state.lock().map_err(|_| invalid())?;
                if state.pending.as_ref().is_some_and(|p| p.id == id) {
                    state.pending = None;
                    Err(Fault::new(
                        "SETTINGS_CONFIRMATION_TIMEOUT",
                        "native Codex did not confirm the permission change",
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }

    pub(crate) async fn authorization(
        &self,
        message: &Value,
    ) -> Result<Option<SessionPermissions>, Fault> {
        let thread = if message["method"] == "process/start" {
            message
                .pointer("/params/env/CODEX_THREAD_ID")
                .and_then(Value::as_str)
        } else {
            None
        };
        if let Some(thread) = thread
            && self.full_access()
            && !self.contains(thread).await?
        {
            return Ok(None);
        }
        let state = self.state.lock().map_err(|_| invalid())?;
        if state.grant.is_none() {
            return Ok(None);
        }
        Ok(state
            .children
            .get(thread.unwrap_or_default())
            .or(state.grant.as_ref())
            .filter(|grant| grant.matches(&state.channel, message))
            .cloned())
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = State::default();
        }
        self.changed.notify_waiters();
    }
}

impl State {
    fn grant(&self) -> SessionPermissions {
        SessionPermissions {
            id: uuid::Uuid::new_v4().to_string(),
            channel: self.channel.clone(),
            thread: self.thread.clone(),
        }
    }
    fn commit(&mut self) {
        if let Some(change) = &self.pending
            && change.acknowledged
            && change.observed
        {
            self.grant = change.full.then(|| self.grant());
            self.pending = None;
        }
    }
}

fn invalid() -> Fault {
    Fault::new(
        "INVALID_SESSION_PERMISSIONS",
        "cannot apply permissions to this bound session",
    )
}
