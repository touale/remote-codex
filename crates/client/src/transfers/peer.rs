use super::{lease::Lease, model::*};
use crate::{ClientError, Result, remote::Remote};
use remote_codex_protocol::{
    Call, Frame, Request, VERSION, codec,
    transfer::{Command, Reply, wire},
};
use std::{process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::BufReader,
    process::{Child, ChildStdin, ChildStdout},
};

pub(super) struct Peer {
    pub root: remote_codex_protocol::transfer::RootIdentity,
    reader: BufReader<ChildStdout>,
    writer: ChildStdin,
    _child: Child,
}
impl Peer {
    pub(super) async fn open(
        remote: &Remote,
        root: &str,
        expected: Option<remote_codex_protocol::transfer::RootIdentity>,
    ) -> Result<Self> {
        let command = format!(
            "exec {} --state {} relay",
            crate::ssh::quote(
                remote
                    .access
                    .service_executable
                    .as_deref()
                    .ok_or(ClientError::RemoteResponse)?
            )?,
            crate::ssh::quote(
                remote
                    .access
                    .service_root
                    .as_deref()
                    .ok_or(ClientError::RemoteResponse)?
            )?
        );
        let mut child = remote
            .ssh
            .command(&remote.server.endpoint, Some(&command), false)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut writer = child.stdin.take().ok_or(ClientError::RemoteResponse)?;
        let mut reader = codec::Reader::new(BufReader::new(
            child.stdout.take().ok_or(ClientError::RemoteResponse)?,
        ));
        codec::write(
            &mut writer,
            &Call {
                protocol: VERSION,
                id: uuid::Uuid::new_v4().to_string(),
                profile: remote.profile.clone(),
                expected_identity: Some(remote.identity.identity.clone()),
                request: Request::Transfer {
                    workspace: root.into(),
                    expected_root: expected,
                },
            },
        )
        .await?;
        match tokio::time::timeout(Duration::from_secs(40), reader.next::<Frame>())
            .await
            .map_err(|_| ClientError::Timeout)??
        {
            Some(Frame::Result(value)) => Ok(Self {
                root: serde_json::from_value(value)?,
                reader: reader.into_inner(),
                writer,
                _child: child,
            }),
            Some(Frame::Error(error)) => Err(error.into()),
            _ => Err(ClientError::RemoteResponse),
        }
    }
    async fn call(&mut self, command: Command, bytes: &[u8]) -> Result<(Reply, Vec<u8>)> {
        tokio::time::timeout(Duration::from_secs(600), async {
            wire::write(&mut self.writer, &command, bytes).await?;
            let (reply, bytes) = wire::read::<
                std::result::Result<Reply, remote_codex_protocol::Fault>,
            >(&mut self.reader)
            .await?;
            Ok((reply?, bytes))
        })
        .await
        .map_err(|_| ClientError::Timeout)?
    }
}
pub(super) struct Peers {
    pub remote: Peer,
    pub locals: Vec<Arc<remote_codex_transfer::Endpoint>>,
    pub lease: Arc<Lease>,
    pub direction: Direction,
}
impl Peers {
    pub(super) async fn new(task: &Task, remote: &Remote, lease: Arc<Lease>) -> Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let locals = task
            .grants
            .iter()
            .map(|grant| {
                let metadata = std::fs::symlink_metadata(&grant.root)?;
                if !metadata.is_dir()
                    || metadata.dev() != grant.device
                    || metadata.ino() != grant.inode
                {
                    return Err(ClientError::Argument(
                        "Local transfer directory moved or changed. Select it again.",
                    ));
                }
                let endpoint = remote_codex_transfer::Endpoint::open(&grant.root, &remote.profile)?;
                let opened = endpoint.identity()?;
                if opened.device != grant.device || opened.inode != grant.inode {
                    return Err(ClientError::Argument(
                        "Local transfer directory changed while opening it.",
                    ));
                }
                Ok(Arc::new(endpoint))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            remote: Peer::open(remote, &task.view.workspace, task.remote_root.clone()).await?,
            locals,
            lease,
            direction: task.view.direction,
        })
    }
    pub(super) async fn call(
        &mut self,
        source: bool,
        root: usize,
        command: Command,
        bytes: Vec<u8>,
    ) -> Result<(Reply, Vec<u8>)> {
        let local = source == (self.direction == Direction::Upload);
        if !local {
            return self.remote.call(command, &bytes).await;
        }
        let endpoint = self
            .locals
            .get(root)
            .cloned()
            .ok_or(ClientError::RemoteResponse)?;
        let lease = self.lease.clone();
        tokio::task::spawn_blocking(move || {
            let _lease = lease;
            endpoint.dispatch(command, &bytes).map_err(Into::into)
        })
        .await
        .map_err(|_| ClientError::Argument("Transfer worker stopped."))?
    }
    pub(super) async fn stat(
        &mut self,
        source: bool,
        root: usize,
        path: &str,
    ) -> Result<Option<remote_codex_protocol::transfer::Stamp>> {
        match self
            .call(
                source,
                root,
                Command::Stat { path: path.into() },
                Vec::new(),
            )
            .await?
            .0
        {
            Reply::Stat { stamp } => Ok(stamp),
            _ => Err(ClientError::RemoteResponse),
        }
    }
}

impl Drop for Peers {
    fn drop(&mut self) {
        for endpoint in &self.locals {
            endpoint.stop();
        }
    }
}
