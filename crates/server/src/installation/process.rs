use crate::Result;
use remote_codex_protocol::Fault;
use std::path::Path;

/// A rejected protocol cannot identify its service. Bind the kernel peer PID
/// to the supervisor and state directory before considering replacement.
#[cfg(target_os = "linux")]
pub(super) fn verify(root: &Path, pid: i32) -> Result<()> {
    use crate::Checked;
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt, path::PathBuf};
    let process = PathBuf::from(format!("/proc/{pid}"));
    let executable = std::fs::read_link(process.join("exe"))
        .checked("SERVICE_UPDATE", "cannot verify running service executable")?;
    let command = std::fs::read(process.join("cmdline"))
        .checked("SERVICE_UPDATE", "cannot verify running service arguments")?;
    let args: Vec<&[u8]> = command
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .collect();
    if executable.file_name() != Some(OsStr::new("remote-codex-server"))
        || args.len() != 4
        || args[1] != b"--state"
        || args[3] != b"serve"
    {
        return Err(Fault::new(
            "SERVICE_UPDATE",
            "socket peer is not a verified execution supervisor",
        ));
    }
    let state = Path::new(OsStr::from_bytes(args[2]));
    let state = if state.is_absolute() {
        state.to_owned()
    } else {
        std::fs::read_link(process.join("cwd"))
            .checked("SERVICE_UPDATE", "cannot verify running service directory")?
            .join(state)
    };
    let state = state.canonicalize().checked(
        "SERVICE_UPDATE",
        "cannot resolve running service state directory",
    )?;
    let expected = root
        .canonicalize()
        .checked("SERVICE_UPDATE", "cannot resolve installation directory")?;
    if state != expected {
        return Err(Fault::new(
            "SERVICE_UPDATE",
            "running service belongs to another installation",
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub(super) fn verify(_root: &Path, _pid: i32) -> Result<()> {
    Err(Fault::new(
        "SERVICE_UPDATE",
        "automatic service replacement requires Linux process information",
    ))
}
