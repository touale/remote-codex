mod relay;

use super::Remote;
use crate::{ClientError, Result};
use remote_codex_protocol::{MAX_FRAME, Request};
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::watch};
use tokio_tungstenite::{
    accept_hdr_async_with_config,
    tungstenite::{
        handshake::server::{Request as Upgrade, Response},
        protocol::WebSocketConfig,
    },
};

/// Capability-addressed loopback endpoint consumed only by the local Codex.
pub(crate) struct Bridge {
    pub(crate) url: String,
    pub(crate) channel: String,
    remote: Arc<Remote>,
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<Result<()>>,
}

impl Bridge {
    pub(crate) async fn start(
        remote: Arc<Remote>,
        revision: i64,
        mcp: Vec<remote_codex_protocol::ExecutionCommand>,
        approvals: Arc<crate::local::approvals::Approvals>,
        permissions: Arc<crate::local::permissions::Permissions>,
    ) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let capability = format!(
            "/{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let url = format!("ws://{}{}", listener.local_addr()?, capability);
        let channel = uuid::Uuid::new_v4().to_string();
        remote
            .call(Request::OpenExecution {
                channel: channel.clone(),
                revision,
                mcp,
            })
            .await?;
        let (stop, mut stopped) = watch::channel(false);
        let connection = remote.clone();
        let id = channel.clone();
        let task = tokio::spawn(async move {
            loop {
                let stream = tokio::select! {
                    incoming = listener.accept() => incoming?.0,
                    _ = stopped.changed() => return Ok(()),
                };
                let expected = capability.clone();
                let handshake = accept_hdr_async_with_config(
                    stream,
                    #[allow(clippy::result_large_err)]
                    // tungstenite fixes the callback error type.
                    move |request: &Upgrade, response: Response| {
                        if request.uri().path() != expected
                            || request.uri().query().is_some()
                            || request.headers().contains_key("origin")
                        {
                            let mut denied = tokio_tungstenite::tungstenite::http::Response::new(
                                Some("Forbidden".into()),
                            );
                            *denied.status_mut() =
                                tokio_tungstenite::tungstenite::http::StatusCode::FORBIDDEN;
                            return Err(denied);
                        }
                        Ok(response)
                    },
                    Some(
                        WebSocketConfig::default()
                            .max_message_size(Some(MAX_FRAME))
                            .max_frame_size(Some(MAX_FRAME)),
                    ),
                );
                let frontend = tokio::select! {
                    handshake = tokio::time::timeout(Duration::from_secs(5), handshake) => match handshake { Ok(Ok(ws)) => ws, _ => continue },
                    _ = stopped.changed() => return Ok(()),
                };
                // An executor has one initialized native transport. Reconnection is
                // handled below that transport, never by replaying initialize.
                return relay::run(
                    frontend,
                    &connection,
                    &id,
                    &mut stopped,
                    &approvals,
                    &permissions,
                )
                .await;
            }
        });
        Ok(Self {
            url,
            channel,
            remote,
            stop,
            task,
        })
    }

    pub(crate) async fn detach(&self) {
        self.close();
        let _ = tokio::time::timeout(
            Duration::from_secs(3),
            self.remote.call(Request::DetachExecution {
                channel: self.channel.clone(),
            }),
        )
        .await;
        self.task.abort();
    }

    pub(crate) fn close(&self) {
        let _ = self.stop.send(true);
        self.task.abort();
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.task.is_finished() {
            Err(ClientError::RemoteFault(
                "EXECUTION_DISCONNECTED".into(),
                "execution environment disconnected; reopen this local session to recover jobs"
                    .into(),
                true,
            ))
        } else {
            Ok(())
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        self.task.abort();
    }
}
