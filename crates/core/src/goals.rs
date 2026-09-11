use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollaborationMode {
    #[default]
    Agent,
    Plan,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Paused,
    Blocked,
    #[serde(alias = "usageLimited")]
    UsageLimited,
    #[serde(alias = "budgetLimited")]
    BudgetLimited,
    Complete,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Goal {
    #[serde(alias = "threadId")]
    pub thread_id: String,
    pub objective: String,
    pub status: GoalStatus,
    #[serde(alias = "tokenBudget")]
    pub token_budget: Option<u64>,
    #[serde(alias = "tokensUsed")]
    pub tokens_used: u64,
    #[serde(alias = "timeUsedSeconds")]
    pub time_used_seconds: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum GoalAction {
    Set {
        objective: String,
        token_budget: Option<u64>,
    },
    Pause,
    Resume,
    Budget {
        token_budget: Option<u64>,
    },
    Clear,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanStep {
    pub step: String,
    pub status: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub turn: String,
    pub explanation: Option<String>,
    pub steps: Vec<PlanStep>,
}
