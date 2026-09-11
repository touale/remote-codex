//! Presentation data supplied by native Codex, never inferred from model prose.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub struct Submission {
    pub turn_id: String,
    pub sent_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TurnTiming {
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TurnState {
    pub id: String,
    pub status: String,
    pub timing: TurnTiming,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenUsage {
    pub last_tokens: u64,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub context_window: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RateWindow {
    pub used_percent: f64,
    pub window_minutes: Option<u64>,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RateLimit {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    pub name: String,
    pub primary: Option<RateWindow>,
    pub secondary: Option<RateWindow>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageAvailability {
    Available,
    SignedOut,
    Unsupported,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountUsage {
    pub account_id: Option<String>,
    pub availability: UsageAvailability,
    pub limits: Vec<RateLimit>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionStatus {
    pub provider: Option<String>,
    pub activity: String,
    pub active_flags: Vec<String>,
    pub usage: Option<TokenUsage>,
    pub limits: Vec<RateLimit>,
    pub limits_error: Option<String>,
    pub limits_updated_at: Option<i64>,
}

impl Default for SessionStatus {
    fn default() -> Self {
        Self {
            provider: None,
            activity: "notLoaded".into(),
            active_flags: Vec::new(),
            usage: None,
            limits: Vec::new(),
            limits_error: None,
            limits_updated_at: None,
        }
    }
}
