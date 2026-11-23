use crate::{
    ClientError, Result,
    ssh::{SshTransport, quote},
    store::{ConnectionRecord, ServerAccess},
};
use remote_codex_protocol::{Call, Frame, Request, VERSION, codec};

use tokio::io::BufReader;

pub(crate) struct Channel {
    pub(crate) reader: codec::Reader<BufReader<crate::ssh::Reader>>,
    pub(crate) writer: crate::ssh::Writer,
    _child: crate::ssh::SshProcess,
    pub(crate) expected_identity: Option<String>,
}

impl Channel {
    pub(crate) async fn open(
        ssh: &SshTransport,
        server: &ConnectionRecord,
        access: &ServerAccess,
    ) -> Result<Self> {
        let command = format!(
            "exec {} --state {} relay",
            quote(
                access
                    .service_executable
                    .as_deref()
                    .ok_or(ClientError::RemoteResponse)?
            )?,
            quote(
                access
                    .service_root
                    .as_deref()
                    .ok_or(ClientError::RemoteResponse)?
            )?
        );
        let mut child = ssh.execute(&server.endpoint, &command).await?;
        drain_diagnostics(child.stderr.take());
        let writer = child.stdin.take().ok_or(ClientError::RemoteResponse)?;
        let reader = codec::Reader::new(BufReader::new(
            child.stdout.take().ok_or(ClientError::RemoteResponse)?,
        ));
        Ok(Self {
            reader,
            writer,
            _child: child,
            expected_identity: None,
        })
    }

    pub(crate) async fn call(
        &mut self,
        profile: &str,
        request: Request,
    ) -> Result<serde_json::Value> {
        let call = Call {
            protocol: VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            profile: profile.into(),
            expected_identity: self.expected_identity.clone(),
            request,
        };
        codec::write(&mut self.writer, &call).await?;
        match self.reader.next::<Frame>().await? {
            Some(Frame::Result(value)) => Ok(value),
            Some(Frame::Error(error)) => Err(error.into()),
            _ => Err(ClientError::RemoteResponse),
        }
    }
}

pub(crate) fn drain_diagnostics(stderr: Option<crate::ssh::Reader>) {
    if let Some(mut stderr) = stderr {
        tokio::spawn(async move {
            let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
        });
    }
}
