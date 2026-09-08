use remote_codex_protocol::{Fault, SessionPermissions};
use serde_json::Value;
use std::sync::Mutex;

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
        sandbox: &Value,
    ) -> Result<(), Fault> {
        let mut state = self.state.lock().map_err(|_| invalid())?;
        state.channel = channel.into();
        state.thread = thread.into();
        state.grant = (sandbox["type"] == "dangerFullAccess").then(|| state.grant());
        Ok(())
    }

    /// Record frontend intent. An expansion needs both a native response and
    /// notification; a reduction stops issuing full-access frames immediately.
    pub(crate) fn begin(&self, params: &Value, supported: bool) -> Result<Option<String>, Fault> {
        let full = if let Some(preset) = params["permissions"].as_str() {
            Some(preset == ":danger-full-access")
        } else {
            params
                .pointer("/sandboxPolicy/type")
                .and_then(Value::as_str)
                .map(|p| p == "dangerFullAccess")
        };
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

    pub(crate) fn observe(&self, event: &Value) -> Result<(), Fault> {
        if event["method"] != "thread/settings/updated" {
            return Ok(());
        }
        let mut state = self.state.lock().map_err(|_| invalid())?;
        if event.pointer("/params/threadId").and_then(Value::as_str) != Some(&state.thread) {
            return Ok(());
        }
        let full = event
            .pointer("/params/threadSettings/sandboxPolicy/type")
            .and_then(Value::as_str)
            == Some("dangerFullAccess");
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

    pub(crate) fn authorization(
        &self,
        message: &Value,
    ) -> Result<Option<SessionPermissions>, Fault> {
        let state = self.state.lock().map_err(|_| invalid())?;
        Ok(state
            .grant
            .as_ref()
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
