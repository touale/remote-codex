//! Serialize opening a thread across windows, including pending trust prompts.
use crate::{
    error::{Error, Result},
    state::{AppState, Event},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::sync::{Mutex as Gate, OwnedMutexGuard};

type Entry = (String, Weak<Gate<()>>);
#[derive(Default)]
pub(crate) struct Openings(Mutex<HashMap<String, Entry>>);
impl Openings {
    pub(crate) async fn acquire(
        &self,
        window: &WebviewWindow,
        id: &str,
    ) -> Result<OwnedMutexGuard<()>> {
        let gate = {
            let mut entries = self.0.lock().map_err(|_| crate::state::unavailable())?;
            entries.retain(|_, (_, gate)| gate.strong_count() > 0);
            if let Some((owner, gate)) = entries
                .get(id)
                .and_then(|(owner, gate)| gate.upgrade().map(|g| (owner.clone(), g)))
            {
                if owner != window.label() {
                    drop(entries);
                    focus(window.app_handle(), &owner, id)?;
                    return Err(Error::new(
                        "SESSION_FOCUSED",
                        "This session is opening in another window.",
                    ));
                }
                gate
            } else {
                let gate = Arc::new(Gate::new(()));
                entries.insert(id.into(), (window.label().into(), Arc::downgrade(&gate)));
                gate
            }
        };
        Ok(gate.lock_owned().await)
    }
}
pub(crate) fn focus(app: &AppHandle, owner: &str, id: &str) -> Result<()> {
    if let Some(window) = app.get_webview_window(owner) {
        window.unminimize()?;
        window.set_focus()?;
        if let Some(context) = app
            .state::<AppState>()
            .windows
            .lock()
            .map_err(|_| crate::state::unavailable())?
            .get(owner)
        {
            context.send(Event::FocusSession { id: id.into() });
        }
    }
    Ok(())
}
pub(crate) struct PreparedOpen {
    pub session: remote_codex_client::application::PreparedSession,
    pub _permit: Option<OwnedMutexGuard<()>>,
}
