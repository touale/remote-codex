use crate::Result;
use remote_codex_protocol::{ExecutionRuntime, Fault};
use std::path::{Path, PathBuf};

/// The client selects an installed package, never an arbitrary server executable.
pub(super) async fn resolve(root: &Path, runtime: &ExecutionRuntime) -> Result<PathBuf> {
    let version = &runtime.version;
    if runtime.platform != "linux-x86_64"
        || version.len() > 96
        || !version.starts_with(|c: char| c.is_ascii_digit())
        || !version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c))
        || runtime.archive_sha256.len() != 64
        || !runtime
            .archive_sha256
            .bytes()
            .all(|c| c.is_ascii_hexdigit())
    {
        return Err(Fault::new(
            "INVALID_RUNTIME",
            "invalid managed execution package reference",
        ));
    }
    let base = root
        .parent()
        .ok_or_else(|| Fault::new("INVALID_RUNTIME", "managed runtime root is unavailable"))?;
    let package = base
        .join("runtimes/codex")
        .join(format!("{version}-{}", runtime.platform));
    let program = package.join("bin/codex");
    let canonical = tokio::fs::canonicalize(&program).await.map_err(|_| {
        Fault::new(
            "RUNTIME_UNAVAILABLE",
            "selected execution package is missing; reconnect to prepare it",
        )
    })?;
    let expected = tokio::fs::canonicalize(base)
        .await
        .map_err(|_| Fault::new("INVALID_RUNTIME", "managed runtime root is unavailable"))?
        .join("runtimes/codex")
        .join(format!("{version}-{}", runtime.platform))
        .join("bin/codex");
    let digest = tokio::fs::read_to_string(package.join(".archive-sha256"))
        .await
        .map_err(|_| {
            Fault::new(
                "INVALID_RUNTIME",
                "execution package has no verification record",
            )
        })?;
    if canonical != expected || digest.trim() != runtime.archive_sha256 {
        return Err(Fault::new(
            "INVALID_RUNTIME",
            "execution package path or verification record changed",
        ));
    }
    Ok(canonical)
}
