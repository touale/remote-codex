use super::*;
use remote_codex_protocol::Fault;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(super) struct Master {
    child: tokio::process::Child,
    diagnostic: Option<tokio::task::JoinHandle<String>>,
}

impl Master {
    pub(super) fn alive(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_none())
    }

    pub(super) async fn close(mut self) -> Result<()> {
        self.stop().await
    }

    pub(super) async fn stop(&mut self) -> Result<()> {
        if self.alive()? {
            self.child.kill().await?;
        }
        Ok(())
    }
}

impl Drop for Master {
    fn drop(&mut self) {
        if let Some(task) = &self.diagnostic {
            task.abort();
        }
    }
}

pub(super) async fn start(
    options: &ConnectOptions,
    endpoint: &SshEndpoint,
    socket: &Path,
    interactive: bool,
) -> Result<Master> {
    let mut command = base_command(options, endpoint);
    let askpass =
        super::askpass::Askpass::prepare(options, endpoint, interactive, &mut command).await?;
    if askpass.is_none() && options.batch {
        command.args(["-o", "BatchMode=yes"]);
    }
    command
        .env("LC_ALL", "C")
        .args([
            "-M",
            "-N",
            "-o",
            "ControlPersist=no",
            "-o",
            "ForkAfterAuthentication=no",
        ])
        .arg("-S")
        .arg(socket)
        .arg("--")
        .arg(endpoint.host())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    if let Some(askpass) = &askpass {
        askpass.bind(child.id().ok_or(ClientError::RemoteResponse)?)?;
    }
    let visible = Arc::new(AtomicBool::new(interactive));
    let display = visible.clone();
    let diagnostic = child.stderr.take().map(|mut stderr| {
        tokio::spawn(async move {
            let mut bytes = Vec::new();
            let mut buffer = [0; 1024];
            while let Ok(count) = stderr.read(&mut buffer).await {
                if count == 0 {
                    break;
                }
                if display.load(Ordering::Acquire) {
                    let _ = std::io::Write::write_all(&mut std::io::stderr(), &buffer[..count]);
                }
                bytes.extend_from_slice(&buffer[..count]);
                if bytes.len() > 8192 {
                    bytes.drain(..bytes.len() - 8192);
                }
            }
            String::from_utf8_lossy(&bytes).into_owned()
        })
    });
    let mut master = Master { child, diagnostic };
    let deadline = tokio::time::Instant::now()
        + Duration::from_secs(if interactive || options.interaction.is_some() {
            180
        } else {
            20
        });
    loop {
        if let Some(status) = master.child.try_wait()? {
            let detail = match master.diagnostic.take() {
                Some(task) => task.await.unwrap_or_default(),
                None => String::new(),
            };
            if askpass
                .as_ref()
                .is_some_and(super::askpass::Askpass::failed)
            {
                return Err(ClientError::Credentials);
            }
            let error = classify(status.code().unwrap_or(255), &detail);
            if askpass.is_some() && error.code() == "SSH_AUTH_REQUIRED" {
                return Err(Fault::new("SSH_SAVED_PASSWORD_REJECTED",
                    "saved SSH password was rejected; update it with remote-codex server auth -n NAME, then reopen or resume the session").into());
            }
            return Err(error);
        }
        if socket.try_exists()? {
            visible.store(false, Ordering::Release);
            return Ok(master);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(ClientError::Timeout);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn classify(status: i32, detail: &str) -> ClientError {
    if detail.contains("REMOTE HOST IDENTIFICATION HAS CHANGED")
        || detail.contains("Host key verification failed")
    {
        Fault::new(
            "SSH_IDENTITY_REQUIRED",
            "SSH host identity could not be verified; review the saved host key before retrying",
        )
        .into()
    } else if detail.contains("Permission denied")
        || detail.contains("Too many authentication failures")
    {
        Fault::new(
            "SSH_AUTH_REQUIRED",
            "SSH authentication is required to restore this connection",
        )
        .into()
    } else {
        ClientError::Ssh(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authentication_and_identity_are_not_network_retries() {
        assert_eq!(
            classify(255, "Permission denied (publickey,password).").code(),
            "SSH_AUTH_REQUIRED"
        );
        assert_eq!(
            classify(255, "Host key verification failed.").code(),
            "SSH_IDENTITY_REQUIRED"
        );
        assert_eq!(classify(255, "Connection timed out").code(), "SSH_FAILED");
    }
}
