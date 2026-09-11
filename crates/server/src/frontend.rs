use crate::{
    Checked, Result,
    execution::{Attachment, Input},
    service::Service,
};
use remote_codex_protocol::{Call, Fault, Frame, Request, codec};
use serde_json::json;
use std::{
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::{io::BufReader, net::UnixStream};

pub(crate) async fn connection(service: Arc<Service>, stream: UnixStream) -> Result<()> {
    let credentials = stream
        .peer_cred()
        .checked("CONNECTION_DENIED", "cannot inspect execution peer")?;
    if credentials.uid() != nix::unistd::geteuid().as_raw() {
        return Err(Fault::new(
            "CONNECTION_DENIED",
            "execution peer has another identity",
        ));
    }
    let (read, mut write) = stream.into_split();
    let mut reader = codec::Reader::new(BufReader::new(read));
    let call: Call = tokio::time::timeout(Duration::from_secs(15), reader.next())
        .await
        .checked("TIMEOUT", "execution handshake timed out")?
        .checked("INVALID_REQUEST", "invalid execution handshake")?
        .ok_or_else(|| Fault::new("CONNECTION_CLOSED", "execution handshake missing"))?;
    if matches!(&call.request, Request::Transfer { .. }) {
        return crate::transfers::serve(&service, &call, reader.into_inner(), write).await;
    }
    let validation = service.validate(&call);
    let streaming = matches!(&call.request, Request::AttachExecution { .. });
    if validation.is_err() || !streaming {
        let result = match validation {
            Ok(()) => service.dispatch(&call).await,
            Err(error) => Err(error),
        };
        let frame = match result {
            Ok(value) => Frame::Result(value),
            Err(error) => Frame::Error(error),
        };
        return codec::write(&mut write, &frame)
            .await
            .checked("CONNECTION_CLOSED", "cannot deliver execution response");
    }
    let result = async {
    let Request::AttachExecution { channel, after } = &call.request else {
        return Ok(());
    };
    if call.expected_identity.as_deref() != Some(&service.store.identity) {
        return Err(Fault::new(
            "CONNECTION_DENIED",
            "execution streaming requires a verified installation identity",
        ));
    }
    let handle = service.execution(&call.profile, channel).await?;
    handle.send(Input::Touch)?;
    let epoch = handle.epoch.fetch_add(1, Ordering::AcqRel) + 1;
    let _attachment = Attachment(handle.clone(), epoch);
    codec::write(
        &mut write,
        &Frame::Result(json!({"channel":channel,"cursor":after})),
    )
    .await
    .checked("CONNECTION_CLOSED", "cannot attach execution")?;
    let mut cursor = *after;
    let mut timer = tokio::time::interval(Duration::from_millis(250));
    loop {
        if handle.epoch.load(Ordering::Acquire) != epoch {
            return Ok(());
        }
        let events = service.store.execution_events(channel, cursor).await?;
        for (next, message) in events {
            codec::write(
                &mut write,
                &Frame::Event {
                    cursor: next,
                    message,
                },
            )
            .await
            .checked("CONNECTION_CLOSED", "execution subscriber disconnected")?;
            cursor = next;
        }
        if !handle.alive.load(Ordering::Acquire) {
            return Err(Fault::unknown("execution backend closed"));
        }
        tokio::select! {
            frame=reader.next::<Frame>()=>match frame.checked("CONNECTION_CLOSED","execution subscriber disconnected")? {
                Some(Frame::Execute{operation,message,approval,permissions})=>{
                    uuid::Uuid::parse_str(&operation).map_err(|_|Fault::new("INVALID_REQUEST","invalid execution operation identity"))?;
                    handle.send(Input::Execute{operation,message,approval,permissions})?;
                }
                Some(Frame::Ack{cursor:ack}) if ack<=cursor=>service.store.acknowledge_events(channel,ack).await?,
                Some(Frame::Heartbeat)=>{handle.send(Input::Touch)?;codec::write(&mut write, &Frame::Heartbeat).await.checked("CONNECTION_CLOSED", "cannot acknowledge heartbeat")?;},
                Some(Frame::Detach)=>{handle.send(Input::Detached)?;return Ok(())},
                None=>return Ok(()),
                _=>return Err(Fault::new("INVALID_REQUEST","unexpected execution frame")),
            },
            _=handle.changed.notified()=>{},
            _=timer.tick()=>{},
        }
    }
    }.await;
    if let Err(error) = &result {
        let _ = codec::write(&mut write, &Frame::Error(error.clone())).await;
    }
    result
}
