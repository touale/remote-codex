use crate::{ClientError, Result};
use std::{io, pin::Pin};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::watch,
};

pub(crate) type Reader = Pin<Box<dyn AsyncRead + Send>>;
pub(crate) type Writer = Pin<Box<dyn AsyncWrite + Send>>;

/// Cancelling a local stream closes its SSH channel, never a remote background job.
pub(crate) struct SshProcess {
    pub stdin: Option<Writer>,
    pub stdout: Option<Reader>,
    pub stderr: Option<Reader>,
    pub(super) cancel: watch::Sender<bool>,
    pub(super) task: tokio::task::JoinHandle<io::Result<i32>>,
}

impl SshProcess {
    pub(crate) async fn wait(&mut self) -> Result<i32> {
        (&mut self.task)
            .await
            .map_err(|_| ClientError::RemoteResponse)?
            .map_err(Into::into)
    }

    #[cfg(unix)]
    pub(super) fn from_child(mut child: tokio::process::Child) -> Result<Self> {
        let stdin: Writer = Box::pin(child.stdin.take().ok_or(ClientError::RemoteResponse)?);
        let stdout: Reader = Box::pin(child.stdout.take().ok_or(ClientError::RemoteResponse)?);
        let stderr: Reader = Box::pin(child.stderr.take().ok_or(ClientError::RemoteResponse)?);
        let (cancel, mut cancelled) = watch::channel(false);
        let task = tokio::spawn(async move {
            tokio::select! {
                status = child.wait() => Ok(status?.code().unwrap_or(255)),
                _ = cancelled.changed() => { child.kill().await?; Ok(255) },
            }
        });
        Ok(Self {
            stdin: Some(stdin),
            stdout: Some(stdout),
            stderr: Some(stderr),
            cancel,
            task,
        })
    }
}

impl Drop for SshProcess {
    fn drop(&mut self) {
        self.cancel.send_replace(true);
    }
}
