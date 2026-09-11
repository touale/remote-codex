use remote_codex_client::application::*;
use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Event {
    Transfer {
        transfer: Transfer,
    },
    FilesDropped {
        token: String,
        names: Vec<String>,
        x: f64,
        y: f64,
    },
    AppPreferencesChanged {
        preferences: crate::commands::app_preferences::AppPreferences,
    },
    FocusSession {
        id: String,
    },
    UiAction {
        action: String,
    },
    Delivery {
        id: String,
        event: Box<Event>,
    },
    AuthenticationEnded {
        id: String,
    },
    Session {
        id: String,
        event: SessionEvent,
    },
    Terminal {
        id: String,
        event: ShellEvent,
    },
    Progress {
        operation: String,
        event: remote_codex_client::progress::PrepareEvent,
    },
    Authentication {
        id: String,
        prompt: AuthenticationPrompt,
    },
    Notice {
        message: String,
    },
    CatalogChanged,
    OperationFinished {
        id: String,
    },
    Resync {
        id: String,
    },
    CloseRequested,
}
