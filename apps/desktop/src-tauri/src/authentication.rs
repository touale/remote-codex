use crate::state::Event;
use remote_codex_client::{ClientError, application::AuthenticationHandler};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tauri::ipc::Channel;
use tokio::sync::oneshot;
use zeroize::Zeroizing;

pub(crate) type Replies = Arc<Mutex<HashMap<String, oneshot::Sender<Option<Zeroizing<String>>>>>>;
struct Pending {
    id: String,
    replies: Replies,
    output: Arc<Mutex<Channel<Event>>>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Ok(mut replies) = self.replies.lock() {
            replies.remove(&self.id);
        }
        if let Ok(channel) = self.output.lock() {
            let _ = channel.send(Event::AuthenticationEnded {
                id: self.id.clone(),
            });
        }
    }
}
pub(crate) fn handler(
    replies: Replies,
    output: Arc<Mutex<Channel<Event>>>,
) -> AuthenticationHandler {
    Arc::new(move |prompt| {
        let output = output.clone();
        let replies = replies.clone();
        Box::pin(async move {
            let id = uuid::Uuid::new_v4().to_string();
            let (sender, receiver) = oneshot::channel();
            let _pending = Pending {
                id: id.clone(),
                replies: replies.clone(),
                output: output.clone(),
            };
            replies
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?
                .insert(id.clone(), sender);
            if let Ok(channel) = output.lock() {
                let _ = channel.send(Event::Authentication { id, prompt });
            }
            Ok(
                tokio::time::timeout(std::time::Duration::from_secs(175), receiver)
                    .await
                    .ok()
                    .and_then(std::result::Result::ok)
                    .flatten(),
            )
        })
    })
}
