use remote_codex_protocol::Fault;
use std::{path::PathBuf, process::Stdio, time::Duration};

/// Uses only the user's installation. Detection never installs a second Codex.
pub async fn discover() -> Result<PathBuf, Fault> {
    let path = std::env::var_os("PATH").ok_or_else(missing)?;
    let program = std::env::split_paths(&path)
        .map(|directory| directory.join("codex"))
        .find(|program| program.is_file())
        .ok_or_else(missing)?;
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new(&program)
            .arg("--version")
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| {
        Fault::new(
            "CODEX_VERSION_TIMEOUT",
            "local Codex did not respond to its version check",
        )
    })?
    .map_err(|_| missing())?;
    let expected = format!("codex-cli {}", super::catalog::VERSION);
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != expected {
        return Err(Fault::new(
            "UNSUPPORTED_CODEX_VERSION",
            &format!(
                "this remote-codex release requires Codex {}; update remote-codex or select a compatible local installation",
                super::catalog::VERSION
            ),
        ));
    }
    program.canonicalize().map_err(|_| missing())
}

fn missing() -> Fault {
    Fault::new(
        "CODEX_NOT_FOUND",
        "install Codex on this computer and make codex available in PATH",
    )
}
