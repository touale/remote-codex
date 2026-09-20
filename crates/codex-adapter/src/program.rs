use remote_codex_protocol::Fault;
mod desktop_path;
mod probe;
mod schema;
pub use probe::{Program, inspect};
use std::{ffi::OsString, path::PathBuf, process::Command};

/// Local process settings, kept separate from the inspected executable identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Launch {
    pub path: PathBuf,
    pub search_path: Option<OsString>,
}

impl Launch {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            search_path: None,
        }
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.path);
        if let Some(path) = &self.search_path {
            command.env("PATH", path);
        }
        command
    }
}

/// Uses only the user's installation. Detection never installs a second Codex.
pub async fn discover() -> Result<Launch, Fault> {
    discover_with_path(None).await
}

async fn discover_with_path(search_path: Option<OsString>) -> Result<Launch, Fault> {
    let path = search_path
        .clone()
        .or_else(|| std::env::var_os("PATH"))
        .ok_or_else(missing)?;
    let program = std::env::split_paths(&path)
        .map(|directory| directory.join("codex"))
        .find(|program| program.is_file())
        .ok_or_else(missing)?;
    validate(Launch {
        path: std::path::absolute(program).map_err(|_| missing())?,
        search_path,
    })
    .await
}

pub async fn discover_desktop(selected: Option<PathBuf>) -> Result<Launch, Fault> {
    let search_path = desktop_path::get().await;
    if let Some(path) = selected {
        return validate(Launch { path, search_path }).await;
    }
    match discover_with_path(search_path.clone()).await {
        Ok(path) => return Ok(path),
        Err(error) if error.code != "CODEX_NOT_FOUND" => return Err(error),
        Err(_) => {}
    }
    // Standard installation locations still work when shell discovery fails
    // or the installer has not added Codex to PATH.
    let mut candidates = vec![
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.insert(0, PathBuf::from(home).join(".local/bin/codex"));
    }
    let path = candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(missing)?;
    validate(Launch { path, search_path }).await
}

pub async fn validate(program: Launch) -> Result<Launch, Fault> {
    inspect(&program).await?;
    Ok(program)
}

fn missing() -> Fault {
    Fault::new(
        "CODEX_NOT_FOUND",
        "install Codex on this computer and make codex available in PATH",
    )
}
