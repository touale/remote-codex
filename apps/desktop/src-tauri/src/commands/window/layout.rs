use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{Manager, WebviewWindow};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Preferences {
    pub sidebar_width: f64,
    pub editor_width: f64,
    pub terminal_height: f64,
    pub tree_split: f64,
    pub codex_program: Option<String>,
    pub selected_workspace: Option<(String, String)>,
    pub sidebar_visible: bool,
    pub editor_visible: bool,
    pub terminal_visible: bool,
    pub workspaces_collapsed: bool,
    pub files_collapsed: bool,
    pub collapsed_nodes: Vec<String>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            sidebar_width: 240.0,
            editor_width: 420.0,
            terminal_height: 240.0,
            tree_split: 0.6,
            codex_program: None,
            selected_workspace: None,
            sidebar_visible: true,
            editor_visible: false,
            terminal_visible: false,
            workspaces_collapsed: false,
            files_collapsed: false,
            collapsed_nodes: Vec::new(),
        }
    }
}
pub(crate) fn preferences_root(window: &WebviewWindow) -> Result<PathBuf> {
    let root = std::env::var_os("REMOTE_CODEX_DESKTOP_STATE_DIR")
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| window.path().app_data_dir())?;
    std::fs::create_dir_all(&root)?;
    Ok(root)
}
pub(super) fn preferences_path(window: &WebviewWindow) -> Result<PathBuf> {
    Ok(preferences_root(window)?.join(format!("window-{}.json", window.label())))
}
pub(crate) fn load_preferences(window: &WebviewWindow) -> Result<Preferences> {
    let path = preferences_path(window)?;
    let preferences = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Preferences::default(),
        Err(error) => return Err(error.into()),
    };
    validate(&preferences)?;
    Ok(preferences)
}
pub(super) fn validate(p: &Preferences) -> Result<()> {
    if !(180.0..=500.0).contains(&p.sidebar_width)
        || !(280.0..=1000.0).contains(&p.editor_width)
        || !(100.0..=700.0).contains(&p.terminal_height)
        || !(0.15..=0.85).contains(&p.tree_split)
    {
        return Err(Error::new("INVALID_PREFERENCES", "Invalid window layout."));
    }
    Ok(())
}
