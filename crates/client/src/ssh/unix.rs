use super::{ConnectOptions, SshTransport, base_command, interaction, master};
use crate::{Result, connection::SshEndpoint};
use std::process::Stdio;
use tokio::process::Command;

impl SshTransport {
    pub(crate) async fn execute(
        &self,
        endpoint: &SshEndpoint,
        command: &str,
    ) -> Result<super::SshProcess> {
        let child = self
            .command(endpoint, Some(command), false)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        super::SshProcess::from_child(child)
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
