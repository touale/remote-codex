use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workspace {
    pub server_id: String,
    pub server: String,
    pub path: String,
    pub used_at: i64,
}
