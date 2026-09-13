//! Scripted loopback Responses fixture. Requests never leave this process.
use crate::ProbeResult;
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
    task::JoinSet,
};

pub const MARKER: &str = "FIXTURE_COMPLETE";

#[derive(Default)]
struct State {
    requests: Vec<Value>,
    replies: VecDeque<Value>,
}
struct Shared {
    state: Mutex<State>,
    changed: Notify,
}

pub struct ModelFixture {
    pub base_url: String,
    shared: Arc<Shared>,
    task: tokio::task::JoinHandle<()>,
}

impl ModelFixture {
    pub async fn start() -> ProbeResult<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base_url = format!("http://{}/v1", listener.local_addr()?);
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            changed: Notify::new(),
        });
        let state = shared.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    result = listener.accept(), if connections.len() < 8 => {
                        let Ok((stream, _)) = result else {break;};
                        let state = state.clone();
                        connections.spawn(async move { let _ = tokio::time::timeout(std::time::Duration::from_secs(30), respond(stream, state)).await; });
                    }
                    _ = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Ok(Self {
            base_url,
            shared,
            task,
        })
    }

    pub fn script(&self, replies: impl IntoIterator<Item = Value>) -> ProbeResult<()> {
        self.shared
            .state
            .lock()
            .map_err(|_| "fixture poisoned")?
            .replies
            .extend(replies);
        Ok(())
    }

    pub fn requests(&self) -> ProbeResult<Vec<Value>> {
        Ok(self
            .shared
            .state
            .lock()
            .map_err(|_| "fixture poisoned")?
            .requests
            .clone())
    }

    pub async fn wait_for(&self, count: usize) -> ProbeResult<()> {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                let changed = self.shared.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if self.requests()?.len() >= count {
                    return Ok::<_, Box<dyn std::error::Error + Send + Sync>>(());
                }
                changed.await;
            }
        })
        .await?
    }
}

impl Drop for ModelFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn respond(mut stream: TcpStream, shared: Arc<Shared>) -> ProbeResult<()> {
    let mut bytes = Vec::new();
    let end = loop {
        let mut buffer = [0; 4096];
        let size = stream.read(&mut buffer).await?;
        if size == 0 {
            return Err("incomplete request".into());
        }
        bytes.extend_from_slice(&buffer[..size]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
        if bytes.len() > 64 * 1024 {
            return Err("oversized request headers".into());
        }
    };
    let headers = std::str::from_utf8(&bytes[..end])?;
    if !headers.starts_with("POST /v1/responses ") {
        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await?;
        return Ok(());
    }
    let length = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then_some(value.trim())
        })
        .ok_or("content length missing")?
        .parse::<usize>()?;
    if length > 8 * 1024 * 1024 {
        return Err("oversized model request".into());
    }
    while bytes.len() < end + length {
        let mut buffer = [0; 8192];
        let size = stream.read(&mut buffer).await?;
        if size == 0 {
            return Err("incomplete request body".into());
        }
        bytes.extend_from_slice(&buffer[..size]);
    }
    let request: Value = serde_json::from_slice(&bytes[end..end + length])?;
    let (index, item) = {
        let mut state = shared.state.lock().map_err(|_| "fixture poisoned")?;
        if state.requests.len() >= 64 {
            return Err("model request budget exceeded".into());
        }
        state.requests.push(request);
        let item = state.replies.pop_front().unwrap_or_else(|| message(MARKER));
        (state.requests.len(), item)
    };
    shared.changed.notify_waiters();
    let data = events(index, item)
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>();
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}", data.len()).as_bytes()).await?;
    Ok(())
}

pub fn message(text: &str) -> Value {
    json!({"type":"message","id":format!("fixture_message_{}", uuid::Uuid::new_v4().simple()),"role":"assistant","status":"completed","content":[{"type":"output_text","text":text,"annotations":[]}]})
}

pub fn function(name: &str, arguments: Value, namespace: Option<&str>) -> Value {
    let id = format!("call_{}", uuid::Uuid::new_v4().simple());
    let mut item = json!({"type":"function_call","id":id,"call_id":id,"name":name,"arguments":arguments.to_string(),"status":"completed"});
    if let Some(namespace) = namespace {
        item["namespace"] = json!(namespace);
    }
    item
}

fn events(index: usize, item: Value) -> Vec<Value> {
    let response = json!({"id":format!("response_{index}"),"object":"response","status":"completed","output":[item],
        "usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2,"input_tokens_details":{"cached_tokens":0}}});
    let mut created = response.clone();
    created["output"] = json!([]);
    created["status"] = json!("in_progress");
    let mut pending = item.clone();
    pending["status"] = json!("in_progress");
    vec![
        json!({"type":"response.created","response":created}),
        json!({"type":"response.output_item.added","output_index":0,"item":pending}),
        json!({"type":"response.output_item.done","output_index":0,"item":item}),
        json!({"type":"response.completed","response":response}),
    ]
}
