use super::{missing, schema};
use remote_codex_protocol::Fault;
use std::{
    collections::HashMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::OnceLock,
    time::Duration,
};
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Program {
    pub path: PathBuf,
    pub version: String,
    identity: Identity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

fn identity(path: &Path) -> Result<Identity, Fault> {
    let value = std::fs::metadata(path).map_err(|_| missing())?;
    if !value.is_file() {
        return Err(missing());
    }
    Ok(Identity {
        device: value.dev(),
        inode: value.ino(),
        size: value.len(),
        modified: (value.mtime(), value.mtime_nsec()),
        changed: (value.ctime(), value.ctime_nsec()),
    })
}

impl Program {
    /// A package upgrade between inspection and process creation must be retried.
    pub fn verify(&self) -> Result<(), Fault> {
        if identity(&self.path)? != self.identity {
            return Err(Fault::new(
                "CODEX_CHANGED",
                "local Codex changed during preparation; retry with the current installation",
            ));
        }
        Ok(())
    }
}

pub async fn inspect(program: &Path) -> Result<Program, Fault> {
    if !program.is_absolute() {
        return Err(missing());
    }
    let path = program.canonicalize().map_err(|_| missing())?;
    let stamp = identity(&path)?;
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Program>>> = OnceLock::new();
    let mut cache = CACHE.get_or_init(Mutex::default).lock().await;
    if let Some(found) = cache.get(&path).filter(|p| p.identity == stamp) {
        return Ok(found.clone());
    }
    let home = tempfile::tempdir().map_err(|_| {
        Fault::new(
            "CODEX_PROBE_FAILED",
            "cannot prepare isolated Codex inspection",
        )
    })?;
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new(&path)
            .arg("--version")
            .env("CODEX_HOME", home.path())
            .current_dir(home.path())
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
    let raw = String::from_utf8_lossy(&output.stdout);
    let version = raw
        .trim()
        .strip_prefix("codex-cli ")
        .filter(|_| output.status.success())
        .ok_or_else(|| {
            Fault::new(
                "INVALID_CODEX_VERSION",
                "local executable did not report a Codex version",
            )
        })?;
    semver::Version::parse(version).map_err(|_| {
        Fault::new(
            "INVALID_CODEX_VERSION",
            "local Codex reported an invalid release version",
        )
    })?;
    schema::inspect(&path, home.path()).await?;
    let found = Program {
        path: path.clone(),
        version: version.into(),
        identity: stamp,
    };
    found.verify()?;
    // Bound this process-local cache even if installations are repeatedly changed.
    if cache.len() >= 16 {
        cache.clear();
    }
    cache.insert(path, found.clone());
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacing_a_program_invalidates_its_inspected_identity()
    -> Result<(), Box<dyn std::error::Error>> {
        let home = tempfile::tempdir()?;
        let path = home.path().join("codex");
        std::fs::write(&path, "old executable")?;
        let program = Program {
            identity: identity(&path)?,
            path: path.clone(),
            version: "1.2.3".into(),
        };
        program.verify()?;
        let next = home.path().join("next");
        std::fs::write(&next, "new executable")?;
        std::fs::rename(next, path)?;
        assert_eq!(
            program
                .verify()
                .err()
                .ok_or("identity change missing")?
                .code,
            "CODEX_CHANGED"
        );
        Ok(())
    }
}
