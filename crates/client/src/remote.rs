pub(crate) mod bridge;
mod channel;
mod configuration;
pub(crate) mod recovery;

use crate::{
    ClientError, Result,
    ssh::{ConnectOptions, SshTransport},
    store::{ConnectionRecord, LocalStore, ServerAccess},
};
pub(crate) use channel::Channel;
use remote_codex_protocol::{Hello, Request};
use std::{path::PathBuf, time::Duration};

pub(crate) struct Remote {
    pub(crate) ssh: SshTransport,
    pub(crate) server: ConnectionRecord,
    pub(crate) access: ServerAccess,
    pub(crate) identity: Hello,
    pub(crate) profile: String,
    reconnect: tokio::sync::Mutex<()>,
}

impl Remote {
    pub(crate) async fn from_transport_with_progress(
        store: &LocalStore,
        server: ConnectionRecord,
        access: ServerAccess,
        ssh: SshTransport,
        progress: &impl Fn(crate::progress::PrepareEvent),
    ) -> Result<Self> {
        let program = access
            .service_executable
            .as_deref()
            .ok_or(ClientError::Argument(
                "server is not initialized; run server add with this name",
            ))?;
        let root = access
            .service_root
            .as_deref()
            .ok_or(ClientError::RemoteResponse)?;
        let deferred = crate::servers::start::ensure(&ssh, &server.endpoint, program, root).await?;
        let remote = Self::attach(store, server, access, ssh).await?;
        if deferred {
            progress(crate::progress::PrepareEvent::Stage(
                crate::progress::PrepareStage::UseRunningService,
            ));
        }
        Ok(remote)
    }

    /// Inspect or configure a running compatible supervisor without replacing
    /// it, so server checks remain available during pending updates.
    pub(crate) async fn connect_running(
        store: &LocalStore,
        server: ConnectionRecord,
        config: Option<PathBuf>,
        batch: bool,
    ) -> Result<Self> {
        let access = store.server_access(&server.id).await?;
        let ssh = SshTransport::connect_with(
            ConnectOptions {
                interaction: crate::ssh::interaction::current(),
                config: config.or_else(|| access.ssh_config.clone()),
                identity_file: access.identity_file.clone(),
                batch,
                credentials: Some(crate::ssh::credentials::Credentials::saved(
                    store, &server.id,
                )),
            },
            &server.endpoint,
        )
        .await?;
        Self::attach(store, server, access, ssh).await
    }

    async fn attach(
        store: &LocalStore,
        server: ConnectionRecord,
        access: ServerAccess,
        ssh: SshTransport,
    ) -> Result<Self> {
        let profile = format!("{}/{}", store.installation_id().await?, server.id);
        let identity: Hello = {
            let mut channel = Channel::open(&ssh, &server, &access).await?;
            serde_json::from_value(channel.call(&profile, Request::Hello).await?)?
        };
        if !remote_codex_adapter::catalog::compatible_service(
            identity.protocol,
            &identity.capabilities,
        ) {
            return Err(ClientError::RemoteFault("SERVICE_UPDATE_REQUIRED".into(),
                "running service is incompatible; finish active work and reconnect to apply the prepared update".into(), false));
        }
        if access
            .remote_identity
            .as_ref()
            .is_some_and(|old| old != &identity.identity)
        {
            return Err(ClientError::RemoteFault("REMOTE_IDENTITY_CHANGED".into(), "remote installation identity changed; verify the server before using saved sessions".into(), false));
        }
        Ok(Self {
            ssh,
            server,
            access,
            identity,
            profile,
            reconnect: tokio::sync::Mutex::new(()),
        })
    }

    pub(crate) async fn channel(&self) -> Result<Channel> {
        let mut channel = Channel::open(&self.ssh, &self.server, &self.access).await?;
        channel.expected_identity = Some(self.identity.identity.clone());
        Ok(channel)
    }

    pub(crate) async fn recover(&self, interactive: bool) -> Result<Hello> {
        self.recover_with(interactive, false).await
    }

    pub(crate) async fn recover_silent(&self) -> Result<Hello> {
        self.recover_with(false, true).await
    }

    async fn recover_with(&self, interactive: bool, silent: bool) -> Result<Hello> {
        let _guard = self.reconnect.lock().await;
        if silent {
            self.ssh
                .ensure_connected_silent(&self.server.endpoint)
                .await?;
        } else {
            self.ssh
                .ensure_connected(&self.server.endpoint, interactive)
                .await?;
        }
        // A healthy supervisor is never replaced as a side effect of reconnect.
        let response = match self.call(Request::Hello).await {
            Ok(value) => value,
            Err(error) if recovery::transient(&error) => {
                crate::servers::start::ensure(
                    &self.ssh,
                    &self.server.endpoint,
                    self.access
                        .service_executable
                        .as_deref()
                        .ok_or(ClientError::RemoteResponse)?,
                    self.access
                        .service_root
                        .as_deref()
                        .ok_or(ClientError::RemoteResponse)?,
                )
                .await?;
                self.call(Request::Hello).await?
            }
            Err(error) => return Err(error),
        };
        let hello: Hello = serde_json::from_value(response)?;
        if hello.identity != self.identity.identity {
            return Err(remote_codex_protocol::Fault::new(
                "REMOTE_IDENTITY_CHANGED",
                "remote installation identity changed; verify the server before restoring work",
            )
            .into());
        }
        if !remote_codex_adapter::catalog::compatible_service(hello.protocol, &hello.capabilities) {
            return Err(remote_codex_protocol::Fault::new(
                "SERVICE_UPDATE_REQUIRED",
                "running execution service does not support safe recovery",
            )
            .into());
        }
        Ok(hello)
    }

    pub(crate) async fn call(&self, request: Request) -> Result<serde_json::Value> {
        let mut channel = self.channel().await?;
        tokio::time::timeout(
            Duration::from_secs(40),
            channel.call(&self.profile, request),
        )
        .await
        .map_err(|_| ClientError::Timeout)?
    }

    pub(crate) async fn synchronize(&self, store: &LocalStore) -> Result<()> {
        let config = configuration::resolve(store, &self.server).await?;
        let revision = config.revision;
        self.call(Request::Configure(config)).await?;
        store
            .acknowledge_config(&self.server.id, revision, &self.identity.identity)
            .await
    }

    pub(crate) async fn close(self) -> Result<()> {
        self.ssh.close().await
    }
}
