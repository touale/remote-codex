pub(crate) mod bridge;
mod channel;
mod configuration;

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
                config: config.or_else(|| access.ssh_config.clone()),
                identity_file: access.identity_file.clone(),
                batch,
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
        })
    }

    pub(crate) async fn channel(&self) -> Result<Channel> {
        let mut channel = Channel::open(&self.ssh, &self.server, &self.access).await?;
        channel.expected_identity = Some(self.identity.identity.clone());
        Ok(channel)
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
        let mut access = self.access.clone();
        access.applied_revision = Some(revision);
        access.remote_identity = Some(self.identity.identity.clone());
        access.health = "ready".into();
        access.checked_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| ClientError::RemoteResponse)?
                .as_secs() as i64,
        );
        store.save_access(&self.server.id, &access).await
    }

    pub(crate) async fn close(self) -> Result<()> {
        self.ssh.close().await
    }
}
