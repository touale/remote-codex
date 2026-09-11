use std::{
    io::Cursor,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

use crate::{ClientError, Result, connection::SshEndpoint};

mod askpass;
mod capture;
pub(crate) mod credentials;
pub(crate) mod interaction;
mod master;
pub(crate) mod target;
mod transfer;

pub(crate) struct SshTransport {
    options: ConnectOptions,
    socket_dir: tempfile::TempDir,
    master: tokio::sync::Mutex<master::Master>,
}

#[derive(Clone, Default)]
pub(crate) struct ConnectOptions {
    pub(crate) interaction: Option<interaction::AuthenticationHandler>,
    pub(crate) config: Option<PathBuf>,
    pub(crate) identity_file: Option<PathBuf>,
    pub(crate) batch: bool,
    pub(crate) credentials: Option<credentials::Credentials>,
}

impl SshTransport {
    pub(crate) fn saved_credentials(&mut self, store: &crate::store::LocalStore, server: &str) {
        self.options.credentials = Some(credentials::Credentials::saved(store, server));
    }

    pub(crate) async fn connect_with(
        mut options: ConnectOptions,
        endpoint: &SshEndpoint,
    ) -> Result<Self> {
        endpoint.validate()?;
        if options.interaction.is_none() {
            options.interaction = interaction::current();
        }
        let socket_dir = tempfile::Builder::new()
            .prefix("rc-ssh-")
            .tempdir_in("/tmp")?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_dir.path(), std::fs::Permissions::from_mode(0o700))?;
        let master = master::start(
            &options,
            endpoint,
            &socket_dir.path().join("control"),
            !options.batch,
        )
        .await?;
        Ok(Self {
            options,
            socket_dir,
            master: tokio::sync::Mutex::new(master),
        })
    }

    pub(crate) async fn ensure_connected(
        &self,
        endpoint: &SshEndpoint,
        interactive: bool,
    ) -> Result<()> {
        self.reconnect(endpoint, interactive, false).await
    }

    pub(crate) async fn ensure_connected_silent(&self, endpoint: &SshEndpoint) -> Result<()> {
        self.reconnect(endpoint, false, true).await
    }

    async fn reconnect(
        &self,
        endpoint: &SshEndpoint,
        interactive: bool,
        silent: bool,
    ) -> Result<()> {
        let mut current = self.master.lock().await;
        let socket = self.socket_dir.path().join("control");
        if current.alive()? && socket.try_exists()? {
            return Ok(());
        }
        current.stop().await?;
        match std::fs::remove_file(&socket) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut options = self.options.clone();
        options.batch = !interactive;
        if silent {
            options.interaction = None;
            options.credentials = None;
        }
        *current = master::start(&options, endpoint, &socket, interactive).await?;
        Ok(())
    }

    pub(crate) fn healthy(&self) -> bool {
        self.master
            .try_lock()
            .is_ok_and(|mut master| master.alive().unwrap_or(false))
            && self.socket_dir.path().join("control").exists()
    }

    pub(crate) async fn close(self) -> Result<()> {
        self.master.into_inner().close().await
    }

    pub(crate) async fn script(&self, endpoint: &SshEndpoint, script: &str) -> Result<String> {
        self.run(endpoint, "sh -s", Cursor::new(script.as_bytes()), |_| {})
            .await
    }

    pub(crate) async fn upload(
        &self,
        endpoint: &SshEndpoint,
        source: &Path,
        destination: &str,
        progress: impl Fn(u64, u64),
    ) -> Result<()> {
        let command = format!("umask 077; set -C; cat > {}", quote(destination)?);
        let file = tokio::fs::File::open(source).await?;
        let total = file.metadata().await?.len();
        self.run(endpoint, &command, file, |bytes| progress(bytes, total))
            .await?;
        Ok(())
    }

    async fn run(
        &self,
        endpoint: &SshEndpoint,
        remote_command: &str,
        input: impl AsyncRead + Unpin,
        progress: impl Fn(u64),
    ) -> Result<String> {
        endpoint.validate()?;
        let mut command = self.command(endpoint, Some(remote_command), false);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().ok_or(ClientError::RemoteResponse)?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or(ClientError::RemoteResponse)?
            .take(1024 * 1024 + 1);
        let exchange = async {
            let sender = async move {
                transfer::copy_with_progress(input, &mut stdin, progress).await?;
                stdin.shutdown().await?;
                Result::Ok(())
            };
            let reader = async move {
                let mut bytes = Vec::new();
                stdout.read_to_end(&mut bytes).await?;
                if bytes.len() > 1024 * 1024 {
                    return Err(ClientError::RemoteResponse);
                }
                String::from_utf8(bytes).map_err(|_| ClientError::RemoteResponse)
            };
            let output = match tokio::try_join!(sender, reader) {
                Ok(((), output)) => output,
                Err(error) => {
                    child.kill().await?;
                    return Err(error);
                }
            };
            let status = child.wait().await?;
            if !status.success() {
                return Err(ClientError::Ssh(status.code().unwrap_or(255)));
            }
            Ok(output)
        };
        match tokio::time::timeout(Duration::from_secs(300), exchange).await {
            Ok(result) => result,
            Err(_) => {
                child.kill().await?;
                Err(ClientError::Timeout)
            }
        }
    }

    pub(crate) fn command(
        &self,
        endpoint: &SshEndpoint,
        remote: Option<&str>,
        terminal: bool,
    ) -> Command {
        let mut command = base_command(&self.options, endpoint);
        command
            .arg("-S")
            .arg(self.socket_dir.path().join("control"))
            .args([
                "-o",
                "ControlMaster=no",
                "-o",
                "BatchMode=yes",
                "-o",
                "ProxyCommand=false",
            ]);
        if terminal {
            command.arg("-tt");
        }
        command.arg("--").arg(endpoint.host());
        if let Some(remote) = remote {
            command.arg(remote);
        }
        command.kill_on_drop(true);
        command
    }
}

fn base_command(options: &ConnectOptions, endpoint: &SshEndpoint) -> Command {
    let mut command = Command::new("ssh");
    command.args([
        "-T",
        "-o",
        "ConnectTimeout=15",
        "-o",
        "ServerAliveInterval=5",
        "-o",
        "ServerAliveCountMax=3",
        "-o",
        "StrictHostKeyChecking=ask",
        "-o",
        "ForwardAgent=no",
    ]);
    if let Some(config) = &options.config {
        command.arg("-F").arg(config);
    }
    if options.batch && options.credentials.is_none() {
        command.args(["-o", "BatchMode=yes"]);
    }
    if let Some(identity) = &options.identity_file {
        command
            .arg("-i")
            .arg(identity)
            .args(["-o", "IdentitiesOnly=yes"]);
    }
    if let Some(port) = endpoint.port() {
        command.arg("-p").arg(port.to_string());
    }
    if let Some(user) = endpoint.user() {
        command.arg("-l").arg(user);
    }
    command
}

/// POSIX shell literal; values are never evaluated as shell source.
pub(crate) fn quote(value: &str) -> Result<String> {
    if value.contains('\0') {
        return Err(ClientError::RemoteResponse);
    }
    Ok(format!("'{}'", value.replace('\'', "'\"'\"'")))
}
