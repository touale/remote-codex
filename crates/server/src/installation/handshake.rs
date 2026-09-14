use crate::{Checked, Result};
use remote_codex_protocol::{Call, Fault, Frame, Hello, Request, VERSION, codec};
use std::{io::ErrorKind, path::Path, time::Duration};
use tokio::{io::BufReader, net::UnixStream};

pub(super) async fn connect(socket: &Path) -> Result<Option<UnixStream>> {
    let stream = match UnixStream::connect(socket).await {
        Ok(stream) => stream,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::NotFound | ErrorKind::ConnectionRefused
            ) =>
        {
            return Ok(None);
        }
        Err(error) => {
            return Err(error).checked("SERVICE_UPDATE", "cannot connect to running service");
        }
    };
    let credentials = stream
        .peer_cred()
        .checked("SERVICE_UPDATE", "cannot verify running service")?;
    if credentials.uid() != nix::unistd::geteuid().as_raw() {
        return Err(Fault::new(
            "SERVICE_UPDATE",
            "running service has another owner",
        ));
    }
    Ok(Some(stream))
}

pub(super) async fn call(
    stream: &mut UnixStream,
    request: Request,
    identity: Option<&str>,
) -> Result<serde_json::Value> {
    codec::write(
        stream,
        &Call {
            protocol: VERSION,
            id: "service-maintenance".into(),
            profile: "maintenance".into(),
            expected_identity: identity.map(str::to_owned),
            request,
        },
    )
    .await
    .checked("SERVICE_UPDATE", "cannot inspect running service")?;
    let frame = tokio::time::timeout(
        Duration::from_secs(3),
        codec::Reader::new(BufReader::new(stream)).next::<Frame>(),
    )
    .await
    .checked("SERVICE_UPDATE", "running service did not respond")?
    .checked("SERVICE_UPDATE", "cannot read running service response")?;
    match frame {
        Some(Frame::Result(value)) => Ok(value),
        Some(Frame::Error(error)) => Err(error),
        None => Err(Fault::new(
            "SERVICE_UPDATE",
            "running service closed the handshake connection",
        )),
        _ => Err(Fault::new(
            "SERVICE_UPDATE",
            "running service returned an unexpected handshake response",
        )),
    }
}

pub(super) async fn hello(stream: &mut UnixStream) -> Result<Hello> {
    let value = call(stream, Request::Hello, None).await?;
    let hello: Hello = serde_json::from_value(value)
        .checked("SERVICE_UPDATE", "invalid running service identity")?;
    if hello.protocol != VERSION || hello.identity.is_empty() || hello.build_id.is_empty() {
        return Err(Fault::new(
            "SERVICE_UPDATE",
            "running service returned an invalid protocol or identity",
        ));
    }
    Ok(hello)
}

/// Readiness requires the expected service, not just a listening socket.
pub async fn ready(socket: &Path, identity: Option<&str>, build: &str) -> Result<bool> {
    let Some(mut stream) = connect(socket).await? else {
        return Ok(false);
    };
    let hello = hello(&mut stream).await?;
    if identity.is_some_and(|expected| expected != hello.identity) {
        return Err(Fault::new(
            "REMOTE_IDENTITY_CHANGED",
            "installation identity changed during service startup",
        ));
    }
    if hello.build_id != build {
        return Err(Fault::new(
            "SERVICE_UPDATE",
            "a different service build started during update",
        ));
    }
    Ok(true)
}
