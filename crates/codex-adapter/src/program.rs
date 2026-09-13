use remote_codex_protocol::Fault;
mod probe;
mod schema;
pub use probe::{Program, inspect};
use std::path::PathBuf;

/// Uses only the user's installation. Detection never installs a second Codex.
pub async fn discover() -> Result<PathBuf, Fault> {
    let path = std::env::var_os("PATH").ok_or_else(missing)?;
    let program = std::env::split_paths(&path)
        .map(|directory| directory.join("codex"))
        .find(|program| program.is_file())
        .ok_or_else(missing)?;
    validate(std::path::absolute(program).map_err(|_| missing())?).await
}

pub async fn discover_desktop(selected: Option<PathBuf>) -> Result<PathBuf, Fault> {
    if let Some(path) = selected {
        return validate(path).await;
    }
    match discover().await {
        Ok(path) => return Ok(path),
        Err(error) if error.code != "CODEX_NOT_FOUND" => return Err(error),
        Err(_) => {}
    }
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
    validate(path).await
}

pub async fn validate(program: PathBuf) -> Result<PathBuf, Fault> {
    inspect(&program).await?;
    Ok(program)
}

fn missing() -> Fault {
    Fault::new(
        "CODEX_NOT_FOUND",
        "install Codex on this computer and make codex available in PATH",
    )
}
