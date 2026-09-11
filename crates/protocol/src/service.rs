use crate::Fault;
use serde::{Deserialize, Serialize};

pub const IDLE_RETIREMENT_CAPABILITY: &str = "idle-retirement-v1";

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ServiceActivity {
    pub channels: i64,
    pub jobs: i64,
    #[serde(default)]
    pub transfers: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", content = "details", rename_all = "snake_case")]
pub enum ServiceStart {
    Ready,
    Waiting(ServiceActivity),
    Failed(Fault),
}

impl ServiceActivity {
    pub fn fault(&self) -> Fault {
        Fault::new(
            "SERVICE_UPDATE_BUSY",
            &format!(
                "remote service update is waiting for {} execution channel(s) {} unfinished job(s), and {} active transfer connection(s)",
                self.channels, self.jobs, self.transfers
            ),
        )
    }
}
