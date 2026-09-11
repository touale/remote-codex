//! Native collaboration/goal wire details stay in the versioned adapter.
use crate::thread::Thread;
use remote_codex_core::{
    goals::{CollaborationMode, Goal, GoalAction, Plan, PlanStep},
    session::SessionEvent,
};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};

pub fn params(thread: &str, action: GoalAction) -> Result<(&'static str, Value), Fault> {
    let mut value = json!({"threadId":thread});
    match action {
        GoalAction::Set {
            objective,
            token_budget,
        } => {
            if objective.trim().is_empty()
                || objective.chars().count() > 4000
                || token_budget == Some(0)
            {
                return Err(Fault::new(
                    "INVALID_GOAL",
                    "Enter a goal of 1–4,000 characters and a positive optional token budget.",
                ));
            }
            value["objective"] = json!(objective);
            value["tokenBudget"] = json!(token_budget);
            value["status"] = json!("active");
        }
        GoalAction::Pause => value["status"] = json!("paused"),
        GoalAction::Resume => value["status"] = json!("active"),
        GoalAction::Budget { token_budget } => {
            if token_budget == Some(0) {
                return Err(Fault::new(
                    "INVALID_GOAL",
                    "The token budget must be positive.",
                ));
            }
            value["tokenBudget"] = json!(token_budget);
        }
        GoalAction::Clear => return Ok(("thread/goal/clear", value)),
    }
    Ok(("thread/goal/set", value))
}
pub fn read(value: &Value) -> Result<Option<Goal>, Fault> {
    serde_json::from_value(value["goal"].clone())
        .map_err(|_| Fault::new("INVALID_NATIVE_GOAL", "Codex returned an invalid goal."))
}
impl Thread {
    pub async fn pause_goal_for_recovery(&self) -> Result<(), Fault> {
        if self
            .goal()
            .await?
            .is_some_and(|g| g.status == remote_codex_core::goals::GoalStatus::Active)
        {
            self.codex
                .engine
                .call(
                    "thread/goal/set",
                    json!({"threadId":self.binding.session.id,"status":"paused"}),
                )
                .await?;
        }
        Ok(())
    }
    pub async fn goal(&self) -> Result<Option<Goal>, Fault> {
        read(
            &self
                .codex
                .engine
                .call(
                    "thread/goal/get",
                    json!({"threadId":self.binding.session.id}),
                )
                .await?,
        )
    }
}
pub fn mode(settings: &Value, mode: CollaborationMode) -> Value {
    json!({"mode":if mode == CollaborationMode::Plan {"plan"} else {"default"},
        "settings":{"model":settings["model"],"reasoning_effort":settings["effort"],"developer_instructions":null}})
}
pub fn event(value: &Value) -> Option<SessionEvent> {
    let p = &value["params"];
    Some(match value["method"].as_str()? {
        "thread/goal/updated" => SessionEvent::GoalChanged {
            goal: read(p).ok()?,
        },
        "thread/goal/cleared" => SessionEvent::GoalChanged { goal: None },
        "turn/plan/updated" => SessionEvent::PlanChanged {
            plan: Plan {
                turn: p["turnId"].as_str()?.into(),
                explanation: p["explanation"].as_str().map(str::to_owned),
                steps: p["plan"]
                    .as_array()?
                    .iter()
                    .filter_map(|s| {
                        Some(PlanStep {
                            step: s["step"].as_str()?.into(),
                            status: s["status"].as_str()?.into(),
                        })
                    })
                    .collect(),
            },
        },
        "item/plan/delta" => SessionEvent::PlanMessage {
            turn_id: p["turnId"].as_str()?.into(),
            item_id: p["itemId"].as_str()?.into(),
            text: p["delta"].as_str()?.into(),
            complete: false,
        },
        "item/completed" if p["item"]["type"] == "plan" => SessionEvent::PlanMessage {
            turn_id: p["turnId"].as_str()?.into(),
            item_id: p["item"]["id"].as_str()?.into(),
            text: p["item"]["text"].as_str()?.into(),
            complete: true,
        },
        _ => return None,
    })
}
