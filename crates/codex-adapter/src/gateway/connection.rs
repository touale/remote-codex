use super::{Fault, Result, transport};
use crate::engine::MAX_MESSAGE_BYTES;
use futures_util::{SinkExt, StreamExt, stream::SplitStream};
use serde_json::Value;
use std::{collections::HashSet, time::Duration};
use tokio::{net::UnixStream, sync::mpsc};
use tokio_tungstenite::{
    WebSocketStream, accept_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

// Transport deadlines bound a single handshake/write, not the SSH retry policy.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
// Bound queued output when the native frontend stops reading.
const OUTPUT_CAPACITY: usize = 256;

/// Keep socket writes out of the accept/read loop: a stalled old reader must
/// never prevent its replacement from completing the WebSocket handshake.
pub(super) struct Connection {
    incoming: SplitStream<WebSocketStream<UnixStream>>,
    outgoing: mpsc::Sender<Message>,
    writer: tokio::task::JoinHandle<Result<()>>,
    failure: Option<Fault>,
    delivered_requests: HashSet<String>,
}

impl Connection {
    pub(super) async fn accept(stream: UnixStream) -> Result<Self> {
        if stream.peer_cred().map_err(transport)?.uid() != nix::unistd::geteuid().as_raw() {
            return Err(Fault::new(
                "FRONTEND_OWNER",
                "frontend must belong to the current user",
            ));
        }
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_MESSAGE_BYTES));
        let socket = tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            accept_async_with_config(stream, Some(config)),
        )
        .await
        .map_err(|_| transport("native frontend handshake timed out"))?
        .map_err(|error| transport(format!("native frontend handshake failed: {error}")))?;
        let (mut sink, incoming) = socket.split();
        let (outgoing, mut messages) = mpsc::channel(OUTPUT_CAPACITY);
        let writer = tokio::spawn(async move {
            while let Some(message) = messages.recv().await {
                tokio::time::timeout(WRITE_TIMEOUT, sink.send(message))
                    .await
                    .map_err(|_| transport("native frontend write timed out"))?
                    .map_err(|error| transport(format!("native frontend write failed: {error}")))?;
            }
            Ok(())
        });
        Ok(Self {
            incoming,
            outgoing,
            writer,
            failure: None,
            delivered_requests: HashSet::new(),
        })
    }

    pub(super) async fn next(&mut self) -> Option<Result<Message>> {
        if let Some(error) = self.failure.take() {
            return Some(Err(error));
        }
        tokio::select! {
            result = &mut self.writer => match result {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(Err(error)),
                Err(error) => Some(Err(transport(format!("native frontend writer failed: {error}")))),
            },
            message = self.incoming.next() => {
                if let Some(Ok(Message::Ping(data))) = &message {
                    self.enqueue(Message::Pong(data.clone()));
                }
                message.map(|result| result.map_err(|error| transport(format!("native frontend read failed: {error}"))))
            }
        }
    }

    pub(super) fn send(&mut self, value: Value) {
        // Snapshot replay and live events share this connection. RPC responses
        // also have IDs, so only deduplicate server requests (method + id).
        if value.get("method").is_some()
            && let Some(id) = value.get("id")
            && !self.delivered_requests.insert(id.to_string())
        {
            return;
        }
        // Keep the ID after the user's answer until the ordered live stream
        // resolves it: an older copy may still be queued ahead of that event.
        if value["method"] == "serverRequest/resolved"
            && let Some(id) = value["params"].get("requestId")
        {
            self.delivered_requests.remove(&id.to_string());
        }
        self.enqueue(Message::Text(value.to_string().into()));
    }

    fn enqueue(&mut self, message: Message) {
        if let Err(mpsc::error::TrySendError::Full(_)) = self.outgoing.try_send(message) {
            self.failure
                .get_or_insert_with(|| transport("native frontend output queue is full"));
        }
        // A closed queue is reported by the writer task with its original error.
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.writer.abort();
    }
}
