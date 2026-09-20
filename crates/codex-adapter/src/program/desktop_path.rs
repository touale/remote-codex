use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use std::{
    ffi::{OsStr, OsString},
    io,
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command, sync::OnceCell};

const START: &[u8] = b"\0remote-codex-path\0";
const END: &[u8] = b"\0remote-codex-end\0";
const MAX_SHELL_OUTPUT: usize = 64 * 1024;
const SCRIPT: &str = "printf '\\000remote-codex-path\\000'; /usr/bin/printenv PATH; printf '\\000remote-codex-end\\000'";

// Dropping a cancelled probe must also stop the shell's subprocesses.
struct ProbeGroup(Option<Pid>);

impl Drop for ProbeGroup {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            let _ = killpg(pid, Signal::SIGKILL);
        }
    }
}

pub(super) async fn get() -> Option<OsString> {
    static PATH: OnceCell<Option<OsString>> = OnceCell::const_new();
    PATH.get_or_init(|| async {
        match resolve().await {
            Ok(path) => Some(path),
            Err(_) => {
                eprintln!("remote-codex: Could not load the login shell PATH; using the inherited PATH. Local MCP tools may be unavailable.");
                None
            }
        }
    }).await.clone()
}

async fn resolve() -> io::Result<OsString> {
    let user = nix::unistd::User::from_uid(nix::unistd::getuid())?
        .ok_or_else(|| io::Error::other("local account unavailable"))?;
    let shell = std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or(user.shell);
    let mut command = Command::new(shell);
    command.args(["-ilc", SCRIPT]).current_dir(user.dir);
    let output = capture(command, Duration::from_secs(3)).await?;
    merge(std::env::var_os("PATH").as_deref(), &output)
}

async fn capture(mut command: Command, timeout: Duration) -> io::Result<Vec<u8>> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()?;
    let mut group = ProbeGroup(
        child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .map(Pid::from_raw),
    );
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("shell output unavailable"))?
        .take(MAX_SHELL_OUTPUT as u64 + 1);
    let result = tokio::time::timeout(timeout, async {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).await?;
        if output.len() > MAX_SHELL_OUTPUT {
            return Err(io::Error::other("shell output too large"));
        }
        if !child.wait().await?.success() {
            return Err(io::Error::other("login shell failed"));
        }
        Ok(output)
    })
    .await
    .unwrap_or_else(|_| {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "login shell timed out",
        ))
    });
    if result.is_err() {
        drop(group);
        let _ = child.kill().await;
    } else {
        group.0 = None;
    }
    result
}

fn merge(inherited: Option<&OsStr>, output: &[u8]) -> io::Result<OsString> {
    let start = output
        .windows(START.len())
        .position(|v| v == START)
        .ok_or_else(|| io::Error::other("shell PATH missing"))?
        + START.len();
    let value = &output[start..];
    let end = value
        .windows(END.len())
        .position(|v| v == END)
        .ok_or_else(|| io::Error::other("shell PATH incomplete"))?;
    let value = value[..end].strip_suffix(b"\n").unwrap_or(&value[..end]);
    if value.is_empty() || value.contains(&0) {
        return Err(io::Error::other("shell PATH invalid"));
    }
    let shell = OsString::from_vec(value.to_vec());
    let mut paths = Vec::new();
    for path in inherited.map(std::env::split_paths).into_iter().flatten() {
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    for path in std::env::split_paths(&shell) {
        // Shell-relative entries must not become executable search paths in a
        // different working directory (such as CODEX_HOME).
        if path.is_absolute() && !paths.contains(&path) {
            paths.push(path);
        }
    }
    std::env::join_paths(paths).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::Engine, program::Launch};
    use std::os::unix::fs::PermissionsExt;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[tokio::test]
    async fn shell_path_reaches_the_local_engine() -> TestResult {
        let home = tempfile::tempdir()?;
        let bin = home.path().join("user tools");
        std::fs::create_dir(&bin)?;
        let tool = bin.join("mcp-launch-probe");
        std::fs::write(&tool, "#!/bin/sh\nprintf ready")?;
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700))?;
        let program = home.path().join("codex");
        std::fs::write(
            &program,
            r#"#!/bin/sh
ready=$(mcp-launch-probe) || exit 1
while IFS= read -r request; do
case "$request" in
  *'"method":"initialize"'*) printf '{"id":1,"result":{"probe":"%s"}}\n' "$ready" ;;
esac
done
"#,
        )?;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))?;
        let mut shell = Command::new("/bin/sh");
        shell.args(["-c", &format!("printf 'startup banner\\n'; export PATH=\"$1\"; {SCRIPT}; printf 'logout banner\\n'"), "fixture"]).arg(&bin).env("PATH", "/usr/bin:/bin");
        let output = capture(shell, Duration::from_secs(3)).await?;
        let search_path = merge(Some(OsStr::new("/usr/bin:/bin")), &output)?;
        let launch = Launch {
            path: program,
            search_path: Some(search_path),
        };
        let (engine, initialized) = Engine::local(&launch, home.path()).await?;
        engine.shutdown().await;
        assert_eq!(initialized["probe"], "ready");
        assert!(Launch::new("codex").command().get_envs().next().is_none());
        Ok(())
    }

    #[test]
    fn merge_preserves_priority_and_rejects_incomplete_output() -> TestResult {
        let output =
            b"banner\0remote-codex-path\0/user tools:/bin:.:/user tools\n\0remote-codex-end\0bye";
        let path = merge(Some(OsStr::new("/preferred:/bin:/preferred")), output)?;
        assert_eq!(path, OsStr::new("/preferred:/bin:/user tools"));
        for output in [
            b"no PATH".as_slice(),
            b"\0remote-codex-path\0partial",
            b"\0remote-codex-path\0\n\0remote-codex-end\0",
        ] {
            assert!(merge(None, output).is_err());
        }
        Ok(())
    }

    #[tokio::test]
    async fn failed_timed_out_and_cancelled_probes_clean_up() -> TestResult {
        let mut failure = Command::new("/bin/sh");
        failure.args(["-c", "read answer; exit 1"]);
        assert!(capture(failure, Duration::from_secs(3)).await.is_err());
        for cancel in [false, true] {
            let home = tempfile::tempdir()?;
            let pids = home.path().join("pids");
            let mut stalled = Command::new("/bin/sh");
            stalled
                .args([
                    "-c",
                    "sleep 30 & printf '%s %s' \"$$\" \"$!\" > \"$1\"; wait",
                    "fixture",
                ])
                .arg(&pids);
            let task = tokio::spawn(capture(stalled, Duration::from_secs(1)));
            let ids = tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if let Ok(raw) = std::fs::read_to_string(&pids) {
                        let ids: Vec<_> = raw
                            .split_whitespace()
                            .filter_map(|v| v.parse::<i32>().ok())
                            .collect();
                        if ids.len() == 2 {
                            break ids;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await?;
            // Keep this fixture bounded even if the regression assertion fails.
            let _cleanup = ProbeGroup(Some(Pid::from_raw(ids[0])));
            if cancel {
                task.abort();
                assert!(task.await.is_err_and(|error| error.is_cancelled()));
            } else {
                assert_eq!(
                    task.await?.err().ok_or("timeout missing")?.kind(),
                    io::ErrorKind::TimedOut
                );
            }
            tokio::time::timeout(Duration::from_secs(3), async {
                while ids
                    .iter()
                    .any(|id| nix::sys::signal::kill(Pid::from_raw(*id), None).is_ok())
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await?;
        }
        Ok(())
    }
}
