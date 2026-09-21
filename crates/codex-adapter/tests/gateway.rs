use futures_util::{SinkExt, StreamExt};
use remote_codex_adapter::gateway::{Backend, Gateway};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::{Notify, broadcast, watch},
};
use tokio_tungstenite::{WebSocketStream, client_async, tungstenite::Message};

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Socket = WebSocketStream<UnixStream>;

struct Session {
    events: broadcast::Sender<Value>,
    revoked: watch::Sender<bool>,
    closed: watch::Sender<Option<Option<String>>>,
    started: Notify,
    release: Notify,
    completed: Notify,
    writes: AtomicUsize,
    approvals: AtomicUsize,
}

impl Session {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            events: broadcast::channel(256).0,
            revoked: watch::channel(false).0,
            closed: watch::channel(None).0,
            started: Notify::new(),
            release: Notify::new(),
            completed: Notify::new(),
            writes: AtomicUsize::new(0),
            approvals: AtomicUsize::new(0),
        })
    }
}

fn approval() -> Value {
    json!({"id":"approval", "method":"item/commandExecution/requestApproval", "params":{"threadId":"same-thread"}})
}

impl Backend for Session {
    async fn request(&self, method: &str, _: Value) -> Result<Value, Fault> {
        if method == "turn/start" || method == "thread/resume" {
            if method == "turn/start" {
                self.writes.fetch_add(1, Ordering::SeqCst);
            }
            self.started.notify_one();
            self.release.notified().await;
            self.completed.notify_one();
        }
        Ok(json!({"method":method,"thread":{"id":"same-thread"}}))
    }
    fn respond(&self, _: Value) -> Result<(), Fault> {
        if self.approvals.fetch_add(1, Ordering::SeqCst) != 0 {
            return Err(Fault::new(
                "STALE_APPROVAL",
                "approval is no longer pending",
            ));
        }
        Ok(())
    }
    fn events(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
    fn pending_requests(&self) -> Result<Vec<Value>, Fault> {
        Ok(if self.approvals.load(Ordering::SeqCst) == 0 {
            vec![approval()]
        } else {
            vec![]
        })
    }
    fn revoked(&self) -> watch::Receiver<bool> {
        self.revoked.subscribe()
    }
    fn closed(&self) -> watch::Receiver<Option<Option<String>>> {
        self.closed.subscribe()
    }
    async fn close(&self) {
        self.closed.send_replace(Some(None));
    }
}

async fn connect(gateway: &Gateway) -> Result<Socket, Box<dyn std::error::Error>> {
    Ok(tokio::time::timeout(Duration::from_secs(2), async {
        let stream = UnixStream::connect(&gateway.socket).await?;
        client_async("ws://localhost/", stream)
            .await
            .map(|(socket, _)| socket)
    })
    .await??)
}

async fn request(socket: &mut Socket, id: u64, method: &str) -> TestResult {
    socket
        .send(Message::Text(
            json!({"id":id,"method":method,"params":{}})
                .to_string()
                .into(),
        ))
        .await?;
    Ok(())
}

async fn receive(socket: &mut Socket) -> Result<Value, Box<dyn std::error::Error>> {
    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await?
        .ok_or("socket closed")??;
    Ok(serde_json::from_str(message.to_text()?)?)
}

#[tokio::test]
async fn reconnect_preserves_requests_and_replays_pending_approvals() -> TestResult {
    let session = Session::new();
    let gateway = Gateway::start(session.clone()).await?;
    let mut first = connect(&gateway).await?;
    session.events.send(approval())?;
    assert_eq!(receive(&mut first).await?, approval());
    request(&mut first, 1, "turn/start").await?;
    tokio::time::timeout(Duration::from_secs(2), session.started.notified()).await?;

    // The native client may leave its original socket open while reconnecting.
    // Reusing a request ID must not deliver the old request's result to this socket.
    let mut second = connect(&gateway).await?;
    request(&mut second, 1, "initialize").await?;
    assert_eq!(
        receive(&mut second).await?["result"]["method"],
        "initialize"
    );
    session.release.notify_one();
    tokio::time::timeout(Duration::from_secs(2), session.completed.notified()).await?;
    session.release.notify_one();
    request(&mut second, 2, "thread/resume").await?;
    assert_eq!(receive(&mut second).await?["id"], 2);
    assert_eq!(receive(&mut second).await?, approval());
    second
        .send(Message::Text(
            json!({"id":"approval","result":{"decision":"accept"}})
                .to_string()
                .into(),
        ))
        .await?;
    request(&mut second, 3, "thread/read").await?;
    assert_eq!(receive(&mut second).await?["id"], 3);
    assert_eq!(session.writes.load(Ordering::SeqCst), 1);
    assert_eq!(session.approvals.load(Ordering::SeqCst), 1);
    assert!(session.closed.borrow().is_none());

    drop(first);
    drop(second);
    let mut third = connect(&gateway).await?;
    request(&mut third, 1, "initialize").await?;
    assert_eq!(
        receive(&mut third).await?["result"]["thread"]["id"],
        "same-thread"
    );
    // Process exit explicitly finishes the gateway, without waiting for reconnect grace.
    tokio::time::timeout(Duration::from_secs(2), gateway.finish()).await??;
    assert!(session.closed.borrow().is_some());
    Ok(())
}

#[tokio::test]
async fn resume_replay_and_live_approvals_are_delivered_once_in_either_order() -> TestResult {
    for live_first in [true, false] {
        let session = Session::new();
        let gateway = Gateway::start(session.clone()).await?;
        let mut frontend = connect(&gateway).await?;
        request(&mut frontend, 1, "thread/resume").await?;
        tokio::time::timeout(Duration::from_secs(2), session.started.notified()).await?;
        if live_first {
            session.events.send(approval())?;
            assert_eq!(receive(&mut frontend).await?, approval());
        }
        session.release.notify_one();
        assert_eq!(receive(&mut frontend).await?["id"], 1);
        if !live_first {
            assert_eq!(receive(&mut frontend).await?, approval());
            session.events.send(approval())?;
        }
        // A notification in the same FIFO stream makes any duplicate observable
        // without timing an idle socket or depending on select! branch order.
        let marker = json!({"method":"test/barrier","params":{}});
        session.events.send(marker.clone())?;
        assert_eq!(receive(&mut frontend).await?, marker);
        frontend
            .send(Message::Text(
                json!({"id":"approval","result":{"decision":"accept"}})
                    .to_string()
                    .into(),
            ))
            .await?;
        request(&mut frontend, 2, "thread/read").await?;
        assert_eq!(receive(&mut frontend).await?["id"], 2);
        assert_eq!(session.approvals.load(Ordering::SeqCst), 1);
        assert!(session.closed.borrow().is_none());

        // Answering must not release the deduplication entry while an older live
        // copy may still be queued. Only the ordered resolution event releases it.
        session.events.send(approval())?;
        session.events.send(marker.clone())?;
        assert_eq!(receive(&mut frontend).await?, marker);
        let resolved = json!({"method":"serverRequest/resolved","params":{"threadId":"same-thread","requestId":"approval"}});
        session.events.send(resolved.clone())?;
        assert_eq!(receive(&mut frontend).await?, resolved);
        session.approvals.store(0, Ordering::SeqCst);
        session.events.send(approval())?;
        assert_eq!(receive(&mut frontend).await?, approval());
        gateway.finish().await?;
    }
    Ok(())
}

#[tokio::test]
async fn stalled_frontend_cannot_block_reconnect_or_shutdown() -> TestResult {
    let session = Session::new();
    let gateway = Gateway::start(session.clone()).await?;
    let mut stalled = connect(&gateway).await?;
    let mut half_open = UnixStream::connect(&gateway.socket).await?;
    half_open.write_all(b"GET / HTTP/1.1\r\n").await?;
    // Active requests must complete while another client leaves its handshake unfinished.
    for id in 1..=3 {
        request(&mut stalled, id, "thread/read").await?;
        assert_eq!(receive(&mut stalled).await?["id"], id);
    }
    drop(half_open);
    // Larger than the Unix socket buffer; the first frontend deliberately does not read.
    session
        .events
        .send(json!({"method":"test/output","params":{"text":"x".repeat(8_000_000)}}))?;
    tokio::time::timeout(Duration::from_secs(2), stalled.get_ref().readable()).await??;
    let mut replacement = connect(&gateway).await?;
    request(&mut replacement, 1, "initialize").await?;
    assert_eq!(receive(&mut replacement).await?["id"], 1);
    session
        .events
        .send(json!({"method":"test/output","params":{"text":"x".repeat(8_000_000)}}))?;
    tokio::time::timeout(Duration::from_secs(2), replacement.get_ref().readable()).await??;
    tokio::time::timeout(Duration::from_secs(2), gateway.finish()).await??;
    assert!(session.closed.borrow().is_some());
    Ok(())
}

#[tokio::test]
async fn event_backlog_does_not_extend_reconnect_deadline_or_hide_handshake_error() -> TestResult {
    let session = Session::new();
    let gateway = Gateway::start(session.clone()).await?;
    let mut frontend = connect(&gateway).await?;
    frontend.close(None).await?;
    // Wait for the server to drop the original socket before advancing its grace period.
    let _ = tokio::time::timeout(Duration::from_secs(2), frontend.next()).await?;
    let mut invalid = UnixStream::connect(&gateway.socket).await?;
    invalid
        .write_all(b"not a websocket request\r\n\r\n")
        .await?;
    let mut discarded = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), invalid.read_to_end(&mut discarded)).await??;

    tokio::time::pause();
    let start = tokio::time::Instant::now();
    let mut closed = gateway.closed.clone();
    for _ in 0..4 {
        tokio::time::advance(Duration::from_secs(5)).await;
        // Exceed the fixture's event channel capacity before yielding to the gateway.
        for index in 0..300 {
            session
                .events
                .send(json!({"method":"test/output", "params":{"index":index}}))?;
        }
        tokio::task::yield_now().await;
    }
    closed.wait_for(|closed| *closed).await?;
    assert!(start.elapsed() <= Duration::from_secs(31));
    let error = gateway
        .finish()
        .await
        .err()
        .ok_or("expected reconnect timeout")?;
    assert!(error.message.contains("handshake failed"), "{error}");
    Ok(())
}
