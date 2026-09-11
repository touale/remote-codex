use crate::{
    error::{Error, Result},
    state::{AppState, Event, unavailable},
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{State, WebviewWindow};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct AppPreferences {
    pub revision: u64,
    pub theme: String,
    pub five_hour_limit: bool,
    pub weekly_limit: bool,
    pub context_usage: bool,
    pub session_tokens: bool,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            revision: 0,
            theme: "system".into(),
            five_hour_limit: true,
            weekly_limit: false,
            context_usage: true,
            session_tokens: false,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AppPreferencesPatch {
    theme: Option<String>,
    five_hour_limit: Option<bool>,
    weekly_limit: Option<bool>,
    context_usage: Option<bool>,
    session_tokens: Option<bool>,
}

fn validate(value: &AppPreferences) -> Result<()> {
    if !["system", "light", "dark"].contains(&value.theme.as_str()) {
        return Err(Error::new("INVALID_PREFERENCES", "Invalid appearance."));
    }
    Ok(())
}

fn save(root: &Path, value: &AppPreferences) -> Result<()> {
    validate(value)?;
    let stage = root.join(format!("app-preferences-{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&stage, serde_json::to_vec(value)?)?;
    if let Err(error) = std::fs::rename(&stage, root.join("app-preferences.json")) {
        let _ = std::fs::remove_file(stage);
        return Err(error.into());
    }
    Ok(())
}

fn load(root: &Path) -> Result<AppPreferences> {
    match std::fs::read(root.join("app-preferences.json")) {
        Ok(bytes) => {
            let value = serde_json::from_slice(&bytes)?;
            validate(&value)?;
            Ok(value)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let value = AppPreferences::default();
            save(root, &value)?;
            Ok(value)
        }
        Err(error) => Err(error.into()),
    }
}

fn apply(value: &AppPreferences, patch: AppPreferencesPatch) -> Result<AppPreferences> {
    let next = AppPreferences {
        revision: value.revision.checked_add(1).ok_or_else(unavailable)?,
        theme: patch.theme.unwrap_or_else(|| value.theme.clone()),
        five_hour_limit: patch.five_hour_limit.unwrap_or(value.five_hour_limit),
        weekly_limit: patch.weekly_limit.unwrap_or(value.weekly_limit),
        context_usage: patch.context_usage.unwrap_or(value.context_usage),
        session_tokens: patch.session_tokens.unwrap_or(value.session_tokens),
    };
    validate(&next)?;
    Ok(next)
}

#[tauri::command]
pub(crate) async fn app_preferences(
    window: WebviewWindow,
    state: State<'_, AppState>,
    patch: Option<AppPreferencesPatch>,
) -> Result<AppPreferences> {
    state.window(&window)?;
    let root = super::window::preferences_root(&window)?;
    let mut cached = state.preferences.lock().map_err(|_| unavailable())?;
    if cached.is_none() {
        *cached = Some(load(&root)?);
    }
    let value = cached.as_ref().ok_or_else(unavailable)?;
    if let Some(patch) = patch {
        let next = apply(value, patch)?;
        save(&root, &next)?;
        *cached = Some(next.clone());
        // Emit while holding the writer lock so windows observe revisions in order.
        if let Ok(windows) = state.windows.lock() {
            for context in windows.values() {
                context.send(Event::AppPreferencesChanged {
                    preferences: next.clone(),
                });
            }
        }
        Ok(next)
    } else {
        Ok(value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_independent_window_changes_survive_reopening() -> Result<()> {
        let root = std::env::temp_dir().join(format!("rc-preferences-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root)?;
        let initial = load(&root)?;
        assert_eq!(initial.theme, "system");
        assert!(initial.five_hour_limit && initial.context_usage);
        assert!(!initial.weekly_limit && !initial.session_tokens);
        let first = apply(&initial, serde_json::from_str(r#"{"weekly_limit":true}"#)?)?;
        let second = apply(&first, serde_json::from_str(r#"{"theme":"light"}"#)?)?;
        save(&root, &second)?;
        let restored = load(&root)?;
        assert!(restored.weekly_limit);
        assert_eq!(restored.theme, "light");
        assert_eq!(restored.revision, 2);
        assert!(apply(&restored, serde_json::from_str(r#"{"theme":"invalid"}"#)?).is_err());
        assert_eq!(load(&root)?.theme, "light");
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
