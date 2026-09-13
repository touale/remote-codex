//! Reuse an explicitly authenticated SSH master without installing test credentials.
//! Master lifetimes and optional askpass challenges are local; remote commands use OpenSSH.
use remote_codex_test_support::ProbeResult;
use std::{ffi::OsString, os::unix::process::CommandExt, path::PathBuf};

pub(super) async fn relay() -> ProbeResult<()> {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if std::env::var_os("REMOTE_CODEX_ACCEPTANCE_OUTAGE")
        .is_some_and(|path| std::path::Path::new(&path).exists())
    {
        std::process::exit(255);
    }
    if args.iter().any(|arg| arg == "-G") {
        return Err(std::process::Command::new("/usr/bin/ssh")
            .args(args)
            .exec()
            .into());
    }
    let control = std::env::var_os("REMOTE_CODEX_ACCEPTANCE_CONTROL");
    let target = std::env::var("REMOTE_CODEX_ACCEPTANCE_TARGET")?;
    let port = std::env::var("REMOTE_CODEX_ACCEPTANCE_PORT")?;
    let separator = args
        .iter()
        .position(|arg| arg == "--")
        .ok_or("SSH separator missing")?;
    let expected = target.rsplit('@').next().ok_or("SSH target missing")?;
    if args.get(separator + 1).and_then(|arg| arg.to_str()) != Some(expected) {
        return Err("fixture refuses another SSH destination".into());
    }
    if args.iter().any(|arg| arg == "-M") {
        let index = args
            .iter()
            .position(|arg| arg == "-S")
            .ok_or("master socket missing")?;
        let socket = args
            .get(index + 1)
            .and_then(|arg| arg.to_str())
            .ok_or("master socket missing")?;
        record(
            serde_json::json!({"kind":"master","pid":std::process::id(),"parent":nix::unistd::getppid().as_raw(),"socket":socket}),
        )?;
    }
    // Test-only challenge through the real graphical askpass broker. This dummy
    // answer is never sent to the remote SSH server or saved in a credential vault.
    if args.iter().any(|arg| arg == "-M")
        && std::env::var_os("REMOTE_CODEX_E2E_AUTH_CHALLENGE")
            .is_some_and(|path| std::path::Path::new(&path).exists())
    {
        let askpass = std::env::var_os("SSH_ASKPASS").ok_or("graphical askpass missing")?;
        let result = tokio::process::Command::new(askpass)
            .arg(format!("{target}'s password: "))
            .output()
            .await?;
        if !result.status.success() || result.stdout != b"fixture-password\n" {
            return Err("fixture authentication was not accepted".into());
        }
        record(serde_json::json!({"kind":"authentication","pid":std::process::id()}))?;
    }
    if let Some(remote) = args.get(separator + 2).and_then(|arg| arg.to_str())
        && remote.ends_with(" relay")
    {
        record(
            serde_json::json!({"pid":std::process::id(),"parent":nix::unistd::getppid().as_raw(),"command":remote}),
        )?;
    }
    let Some(control) = control else {
        return Err(std::process::Command::new("/usr/bin/ssh")
            .args(args)
            .exec()
            .into());
    };
    if args.iter().any(|arg| arg == "-M") {
        let status = tokio::process::Command::new("/usr/bin/ssh")
            .arg("-S")
            .arg(&control)
            .args(["-O", "check", "-p", &port, &target])
            .status()
            .await?;
        if !status.success() {
            return Err("authenticated SSH master is unavailable".into());
        }
        let index = args
            .iter()
            .position(|arg| arg == "-S")
            .ok_or("master socket missing")?;
        let socket = PathBuf::from(args.get(index + 1).ok_or("master socket missing")?);
        let _listener = tokio::net::UnixListener::bind(socket)?;
        std::future::pending::<()>().await;
        return Ok(());
    }
    let mut command = std::process::Command::new("/usr/bin/ssh");
    if args.iter().any(|arg| arg == "-tt" || arg == "-t") {
        command.arg("-tt");
    }
    command.arg("-S").arg(control).args([
        "-p",
        &port,
        "-o",
        "BatchMode=yes",
        "-o",
        "ForwardAgent=no",
        "--",
        &target,
    ]);
    command.args(&args[separator + 2..]);
    Err(command.exec().into())
}

pub(super) async fn interrupt_master(parent: u32) -> ProbeResult<()> {
    let log = std::env::var_os("REMOTE_CODEX_ACCEPTANCE_RELAYS").ok_or("master log missing")?;
    for line in std::fs::read_to_string(log)?.lines().rev() {
        let entry: serde_json::Value = serde_json::from_str(line)?;
        if entry["kind"] != "master" || entry["parent"] != parent {
            continue;
        }
        let pid = entry["pid"]
            .as_i64()
            .and_then(|p| i32::try_from(p).ok())
            .ok_or("invalid master PID")?;
        let socket = entry["socket"].as_str().ok_or("missing control path")?;
        let output = tokio::process::Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "ppid=,command="])
            .output()
            .await?;
        let process = String::from_utf8_lossy(&output.stdout);
        if !process.trim_start().starts_with(&format!("{parent} ")) || !process.contains(socket) {
            continue;
        }
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid),
            nix::sys::signal::Signal::SIGTERM,
        )?;
        return Ok(());
    }
    Err("no owned SSH master to interrupt".into())
}

fn record(entry: serde_json::Value) -> ProbeResult<()> {
    use std::io::Write;
    let path = std::env::var_os("REMOTE_CODEX_ACCEPTANCE_RELAYS").ok_or("relay log missing")?;
    // Format before appending so concurrent SSH processes cannot interleave fields.
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(format!("{entry}\n").as_bytes())?;
    Ok(())
}
