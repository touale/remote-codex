use super::{LocalRuntime, route};
use remote_codex_adapter::gateway::Backend;
use remote_codex_protocol::Fault;
use serde_json::Value;
use tokio::sync::{broadcast, watch};

impl Backend for LocalRuntime {
    async fn request(&self, method: &str, params: Value) -> Result<Value, Fault> {
        route::request(self, method, params).await
    }

    fn respond(&self, response: Value) -> Result<(), Fault> {
        LocalRuntime::respond(self, response).map_err(|error| match error {
            crate::ClientError::RemoteFault(code, message, outcome_unknown) => Fault {
                code,
                message,
                outcome_unknown,
            },
            _ => Fault::new("APPROVAL_RESPONSE", &error.to_string()),
        })
    }

    fn events(&self) -> broadcast::Receiver<Value> {
        self.native_events.subscribe()
    }
    fn revoked(&self) -> watch::Receiver<bool> {
        self.lease.revoked.clone()
    }
    fn closed(&self) -> watch::Receiver<Option<Option<String>>> {
        self.closed.subscribe()
    }
    async fn close(&self) {
        self.shutdown().await;
    }
}
