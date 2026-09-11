use crate::state::{Event, WindowState};
use std::sync::atomic::Ordering;
use tokio::sync::oneshot;
impl WindowState {
    /// Each producer has at most one unacknowledged event in the WebView.
    pub(crate) async fn deliver(&self, event: Event) -> bool {
        let owner = match &event {
            Event::Session { id, .. } | Event::Terminal { id, .. } | Event::Resync { id } => id,
            _ => return false,
        };
        let id = format!("{owner}:{}", uuid::Uuid::new_v4());
        let (sender, receiver) = oneshot::channel();
        {
            let Ok(mut pending) = self.deliveries.lock() else {
                return false;
            };
            if self.closing.load(Ordering::Acquire) {
                return false;
            }
            pending.insert(id.clone(), sender);
        }
        let sent = self.events.lock().ok().is_some_and(|channel| {
            channel
                .send(Event::Delivery {
                    id: id.clone(),
                    event: Box::new(event),
                })
                .is_ok()
        });
        if !sent {
            if let Ok(mut pending) = self.deliveries.lock() {
                pending.remove(&id);
            }
            return false;
        }
        receiver.await.is_ok()
    }

    pub(crate) fn close_stream(&self, id: &str) {
        if let Ok(mut streams) = self.streams.lock()
            && let Some(stream) = streams.remove(id)
        {
            stream.abort();
        }
        if let Ok(mut deliveries) = self.deliveries.lock() {
            let prefix = format!("{id}:");
            deliveries.retain(|key, _| !key.starts_with(&prefix));
        }
    }
}
