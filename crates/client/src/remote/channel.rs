use crate::{
    ClientError, Result,
    ssh::{SshTransport, quote},
    store::{ConnectionRecord, ServerAccess},
};
use remote_codex_protocol::{Call, Frame, Request, VERSION, codec};
use std::process::Stdio;
use tokio::{
    io::BufReader,
    process::{Child, ChildStdin, ChildStdout},
};

pub(crate) struct Channel {
    pub(crate) reader: codec::Reader<BufReader<ChildStdout>>,
    pub(crate) writer: ChildStdin,
    _child: Child,
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
        let mut child = ssh
            .command(&server.endpoint, Some(&command), false)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
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
