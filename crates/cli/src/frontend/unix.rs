use super::*;
#[path = "authentication.rs"]
mod authentication;
pub(super) async fn attach(
    gateway: &mut TerminalAttachment,
    interrupt: &mut tokio::signal::unix::Signal,
) -> Result<()> {
    let terminal = authentication::Terminal::capture();
    let mut sessions = gateway.current.clone();
    let mut runtime = sessions.borrow_and_update().clone();
    let mut environment = runtime.environment();
    let mut auth_attempted = false;
    let mut closed = gateway.closed.clone();
    let mut child = gateway
        .command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()?;
    let status = loop {
        tokio::select! {
          changed=sessions.changed()=>{
              if changed.is_ok() {
                  runtime = sessions.borrow_and_update().clone();
                  environment = runtime.environment();
                  auth_attempted = false;
              }
          },
          result=child.wait()=>break result?,
          _=closed.changed()=>break wait_for_exit(&mut child).await?,
          _=interrupt.recv()=>{
              if let Some(pid) = child.id().and_then(|pid| i32::try_from(pid).ok()) {
                  let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), nix::sys::signal::Signal::SIGINT);
              }
              break wait_for_exit(&mut child).await?;
          },
          changed=environment.changed()=>{
              if changed.is_err() { continue; }
              let state = environment.borrow_and_update().clone();
              if matches!(&state, remote_codex_client::session::EnvironmentState::Ready) { auth_attempted = false; }
              if matches!(&state, remote_codex_client::session::EnvironmentState::ActionRequired { code, .. } if code == "SSH_AUTH_REQUIRED") && !auth_attempted {
                  auth_attempted = true;
                  if let Some(pid) = child.id() && let Some(_suspended) = terminal.suspend(pid)? {
                      eprintln!("\nSSH authentication is required to restore this session.");
                      let result = tokio::select! {
                          result=runtime.authenticate()=>result,
                          _=interrupt.recv()=>Err(ClientError::Argument("authentication cancelled")),
                      };
                      if let Err(error) = result { eprintln!("{error}. Exit and resume this session to try again."); }
                  }
              }
          },
        }
    };
    if !status.success() {
        return Err(ClientError::CodexExit(
            u8::try_from(status.code().unwrap_or(1)).unwrap_or(1),
        ));
    }
    Ok(())
}

async fn wait_for_exit(
    child: &mut tokio::process::Child,
) -> std::io::Result<std::process::ExitStatus> {
    // Closing the frontend socket can precede its normal process exit. Allow
    // terminal cleanup to finish before killing an unresponsive frontend.
    match tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await {
        Ok(status) => status,
        Err(_) => {
            child.kill().await?;
            child.wait().await
        }
    }
}
