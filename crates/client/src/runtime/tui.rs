use super::{CANDIDATE_VERSION, download};
use crate::{ClientError, Result, progress::PrepareEvent, store::private_directory};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::Command;

/// Probe only an isolated CODEX_HOME; never discover local history or credentials.
pub async fn program(directory: &Path, progress: impl Fn(PrepareEvent)) -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join("codex");
            if candidate.is_file() {
                if compatible(&candidate).await {
                    return Ok(candidate.canonicalize()?);
                }
                return Err(ClientError::Unsupported(
                    "the Codex found in PATH is outside this distribution's validated version; update remote-codex or install the validated Codex version",
                ));
            }
        }
    }
    let (platform, url, digest) = asset()?;
    let runtimes = directory.join("runtimes");
    private_directory(&runtimes)?;
    let target = runtimes.join(format!("codex-{CANDIDATE_VERSION}-{platform}"));
    let program = target.join("bin/codex");
    if target.exists() {
        if tokio::fs::read_to_string(target.join(".archive-sha256"))
            .await?
            .trim()
            != digest
            || !compatible(&program).await
        {
            return Err(ClientError::Integrity);
        }
        return Ok(program);
    }
    let archive =
        download::verified_package(&directory.join("cache/runtime"), url, digest, progress).await?;
    let stage = tempfile::Builder::new()
        .prefix(".codex-stage-")
        .tempdir_in(&runtimes)?;
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(stage.path())
        .stdin(Stdio::null())
        .status()
        .await?;
    if !status.success() || !compatible(&stage.path().join("bin/codex")).await {
        return Err(ClientError::Integrity);
    }
    tokio::fs::write(stage.path().join(".archive-sha256"), digest).await?;
    match tokio::fs::rename(stage.path(), &target).await {
        Ok(()) => Ok(program),
        Err(_) if target.exists() && compatible(&program).await => Ok(program),
        Err(error) => Err(error.into()),
    }
}

async fn compatible(program: &Path) -> bool {
    let Ok(home) = tempfile::tempdir() else {
        return false;
    };
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new(program)
            .arg("--version")
            .env("CODEX_HOME", home.path())
            .stdin(Stdio::null())
            .output(),
    )
    .await;
    matches!(result,Ok(Ok(output)) if output.status.success() && String::from_utf8_lossy(&output.stdout).trim()==format!("codex-cli {CANDIDATE_VERSION}"))
}

fn asset() -> Result<(&'static str, &'static str, &'static str)> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Ok((
            "macos-aarch64",
            "https://github.com/openai/codex/releases/download/rust-v0.153.4/codex-package-aarch64-apple-darwin.tar.gz",
            "35438da1fbf7a6db7ddb3bcec84448fa6015ba188461472a97d9d1da7d9c4353",
        ))
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Ok(("linux-x86_64", super::ASSET_URL, super::LINUX_X86_64_SHA256))
    } else {
        Err(ClientError::Unsupported(
            "managed TUI package for this local platform",
        ))
    }
}
