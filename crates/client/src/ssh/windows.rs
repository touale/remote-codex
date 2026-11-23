mod authentication;
pub(super) mod config;
mod host_key;
mod stream;
use super::{ConnectOptions, SshProcess, SshTransport};
use crate::{ClientError, Result, connection::SshEndpoint};
use russh::client;
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

type Handle = client::Handle<host_key::Verifier>;
pub(super) struct Connection(Mutex<Handle>);

pub(super) fn failure(error: impl std::fmt::Display) -> ClientError {
    remote_codex_protocol::Fault {
        code: "SSH_FAILED".into(),
        message: format!("SSH connection failed: {error}"),
        outcome_unknown: true,
    }
    .into()
}

async fn connect(
    options: &ConnectOptions,
    endpoint: &SshEndpoint,
    interactive: bool,
) -> Result<Handle> {
    let resolved = config::Config::load(options, endpoint)?;
    let mut options = options.clone();
    if !interactive {
        options.interaction = None;
    }
    let verifier = host_key::Verifier::new(&resolved, options.interaction.clone());
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(5)),
        keepalive_max: 3,
        ..Default::default()
    });
    let socket = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::net::TcpStream::connect((resolved.target.hostname.as_str(), resolved.target.port)),
    )
    .await
    .map_err(|_| ClientError::Timeout)??;
    // Host identity confirmation is interactive; the TCP connection deadline must
    // not expire while the user is reviewing the fingerprint.
    let mut handle = tokio::time::timeout(
        Duration::from_secs(300),
        client::connect_stream(config, socket, verifier),
    )
    .await
    .map_err(|_| ClientError::Timeout)?
    .map_err(failure)?;
    authentication::authenticate(&mut handle, &resolved, &options, endpoint).await?;
    Ok(handle)
}

impl SshTransport {
    pub(crate) async fn connect_with(
        mut options: ConnectOptions,
        endpoint: &SshEndpoint,
    ) -> Result<Self> {
        endpoint.validate()?;
        if options.interaction.is_none() {
            options.interaction = super::interaction::current();
        }
        let handle = connect(&options, endpoint, !options.batch).await?;
        Ok(Self {
            options,
            native: Connection(Mutex::new(handle)),
        })
    }
    pub(crate) async fn ensure_connected(
        &self,
        endpoint: &SshEndpoint,
        interactive: bool,
    ) -> Result<()> {
        let mut handle = self.native.0.lock().await;
        if handle.is_closed() {
            *handle = connect(&self.options, endpoint, interactive).await?;
        }
        Ok(())
    }
    pub(crate) async fn ensure_connected_silent(&self, endpoint: &SshEndpoint) -> Result<()> {
        self.ensure_connected(endpoint, false).await
    }
    pub(crate) fn healthy(&self) -> bool {
        self.native.0.try_lock().is_ok_and(|h| !h.is_closed())
    }
    pub(crate) async fn close(self) -> Result<()> {
        self.native
            .0
            .into_inner()
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await
            .map_err(failure)
    }
    pub(crate) async fn execute(
        &self,
        _endpoint: &SshEndpoint,
        command: &str,
    ) -> Result<SshProcess> {
        let channel = self
            .native
            .0
            .lock()
            .await
            .channel_open_session()
            .await
            .map_err(failure)?;
        channel.exec(true, command).await.map_err(failure)?;
        Ok(stream::process(channel))
    }
    pub(crate) async fn pty(
        &self,
        command: &str,
        columns: u16,
        rows: u16,
    ) -> Result<russh::Channel<client::Msg>> {
        let channel = self
            .native
            .0
            .lock()
            .await
            .channel_open_session()
            .await
            .map_err(failure)?;
        channel
            .request_pty(
                true,
                "xterm-256color",
                u32::from(columns),
                u32::from(rows),
                0,
                0,
                &[],
            )
            .await
            .map_err(failure)?;
        channel.exec(true, command).await.map_err(failure)?;
        Ok(channel)
    }
}
