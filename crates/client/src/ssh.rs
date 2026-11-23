use crate::{ClientError, Result, connection::SshEndpoint};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWriteExt};

#[cfg(unix)]
mod askpass;
mod capture;
pub(crate) mod credentials;
pub(crate) mod interaction;
#[cfg(unix)]
mod master;
mod process;
pub(crate) mod target;
mod transfer;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub(crate) use process::{Reader, SshProcess, Writer};

pub(crate) struct SshTransport {
    options: ConnectOptions,
    #[cfg(unix)]
    socket_dir: tempfile::TempDir,
    #[cfg(unix)]
    master: tokio::sync::Mutex<master::Master>,
    #[cfg(windows)]
    native: windows::Connection,
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
        let mut process = self.execute(endpoint, remote_command).await?;
        let mut stdin = process.stdin.take().ok_or(ClientError::RemoteResponse)?;
        let stdout = process.stdout.take().ok_or(ClientError::RemoteResponse)?;
        let stderr = process.stderr.take().ok_or(ClientError::RemoteResponse)?;
        let operation = async {
            let send = async {
                transfer::copy_with_progress(input, &mut stdin, progress).await?;
                stdin.shutdown().await?;
                Result::Ok(())
            };
            let (_, output, diagnostic) =
                tokio::try_join!(send, capture::bounded(stdout), capture::bounded(stderr))?;
            let code = process.wait().await?;
            if code != 0 {
                return Err(ClientError::Ssh(code));
            }
            if !diagnostic.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&diagnostic));
            }
            String::from_utf8(output).map_err(|_| ClientError::RemoteResponse)
        };
        tokio::time::timeout(Duration::from_secs(300), operation)
            .await
            .map_err(|_| ClientError::Timeout)?
    }
}

#[cfg(unix)]
fn base_command(options: &ConnectOptions, endpoint: &SshEndpoint) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("ssh");
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
