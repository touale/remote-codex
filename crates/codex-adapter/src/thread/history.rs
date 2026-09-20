use super::{Codex, binding};
use remote_codex_core::session::{HistoryItem, HistoryPage, HistoryTurn, SessionBinding};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::path::Path;

pub async fn update(
    program: &crate::program::Launch,
    bound: &SessionBinding,
    name: Option<&str>,
    archived: Option<bool>,
) -> Result<(), Fault> {
    let codex = Codex::start(program, Path::new(&bound.codex_home)).await?;
    let result = async {
        if let Some(name) = name {
            codex
                .engine
                .call(
                    "thread/name/set",
                    json!({"threadId":bound.session.id,"name":name}),
                )
                .await?;
        }
        if let Some(archived) = archived {
            codex
                .engine
                .call(
                    if archived {
                        "thread/archive"
                    } else {
                        "thread/unarchive"
                    },
                    json!({"threadId":bound.session.id}),
                )
                .await?;
        }
        Ok(())
    }
    .await;
    codex.shutdown().await;
    result
}

pub async fn read(
    program: &crate::program::Launch,
    bound: &SessionBinding,
    cursor: Option<&str>,
) -> Result<HistoryPage, Fault> {
    let codex = Codex::start(program, Path::new(&bound.codex_home)).await?;
    let result = page(&codex, bound, cursor).await;
    codex.shutdown().await;
    result
}

pub(super) async fn page(
    codex: &Codex,
    bound: &SessionBinding,
    cursor: Option<&str>,
) -> Result<HistoryPage, Fault> {
    let summary = codex
        .engine
        .call(
            "thread/read",
            json!({"threadId":bound.session.id,"includeTurns":false}),
        )
        .await?;
    let turns = codex
        .engine
        .call(
            "thread/turns/list",
            json!({"threadId":bound.session.id,"cursor":cursor,"itemsView":"full","limit":20}),
        )
        .await?;
    let entries = turns["data"].as_array().ok_or_else(|| {
        Fault::new(
            "INVALID_NATIVE_HISTORY",
            "native history did not contain turns",
        )
    })?;
    Ok(HistoryPage {
        session: binding::session(&summary["thread"], &bound.session.cwd)?,
        turns: entries.iter().map(turn).collect(),
        next_cursor: turns["nextCursor"].as_str().map(str::to_owned),
    })
}

fn turn(value: &Value) -> HistoryTurn {
    HistoryTurn {
        id: text(value, "id"),
        status: text(value, "status"),
        timing: crate::status::timing(value),
        items: value["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| HistoryItem {
                id: text(item, "id"),
                client_id: item["clientId"].as_str().map(str::to_owned),
                sent_at: None,
                phase: item["phase"].as_str().map(str::to_owned),
                kind: text(item, "type"),
                text: item_text(item),
                delivery: item["delivery"].as_str().map(str::to_owned),
                questions: questions(item),
                tool: crate::desktop::tool(item).map(|mut tool| {
                    if tool.status.is_empty() && value["status"] == "completed" {
                        tool.status = "completed".into();
                    }
                    tool
                }),
            })
            .collect(),
    }
}

pub(crate) fn item_text(item: &Value) -> String {
    if let Some(value) = item["text"].as_str() {
        return value.into();
    }
    if item["type"] == "commandExecution" {
        return format!(
            "{}\n{}",
            text(item, "command"),
            text(item, "aggregatedOutput")
        );
    }
    for field in ["content", "summary"] {
        if let Some(parts) = item[field].as_array() {
            return parts
                .iter()
                .filter_map(|part| part.as_str().or_else(|| part["text"].as_str()))
                .collect::<Vec<_>>()
                .join("\n");
        }
    }
    String::new()
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().into()
}

pub(crate) fn questions(item: &Value) -> Vec<remote_codex_core::session::AsyncQuestion> {
    item["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|question| {
            Some(remote_codex_core::session::AsyncQuestion {
                title: question["title"].as_str()?.into(),
                options: question["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            })
        })
        .collect()
}

impl super::Thread {
    pub async fn history(&self) -> Result<HistoryPage, Fault> {
        page(&self.codex, &self.binding, None).await
    }

    pub async fn revert(&self, before_turn: &str) -> Result<(), Fault> {
        self.codex
            .engine
            .call(
                "thread/revert",
                json!({
                    "threadId": self.binding.session.id, "beforeTurnId": before_turn
                }),
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_codex_core::session::SessionEvent;

    #[test]
    fn tool_details_and_completion_survive_live_and_history_projection()
    -> Result<(), Box<dyn std::error::Error>> {
        let item = json!({"id":"search","type":"webSearch","query":"Rust async",
            "action":{"type":"search","queries":["Rust async"]},
            "results":[{"title":"Rust","url":"https://www.rust-lang.org/"}]});
        for (method, expected) in [
            ("item/started", "inProgress"),
            ("item/completed", "completed"),
        ] {
            let event = crate::events::public_event(&json!({"method":method,
                "params":{"turnId":"turn","item":item}}))
            .ok_or("missing event")?;
            let SessionEvent::ToolChanged { item: live, .. } = event else {
                return Err("expected tool event".into());
            };
            assert_eq!(live.status, expected);
            if method == "item/completed" {
                let history = turn(&json!({"id":"turn","status":"completed","items":[item]}));
                assert_eq!(
                    serde_json::to_value(&history.items[0].tool)?,
                    serde_json::to_value(live)?
                );
            }
        }
        let history = turn(&json!({"id":"turn","status":"interrupted","items":[item]}));
        assert!(
            history.items[0]
                .tool
                .as_ref()
                .ok_or("missing tool")?
                .status
                .is_empty()
        );
        Ok(())
    }

    #[test]
    fn asynchronous_choices_survive_live_and_history_projection()
    -> Result<(), Box<dyn std::error::Error>> {
        let item = json!({"type":"agentMessage","id":"question","text":"Choose a scope",
            "delivery":"async","questions":[{"title":"Which scope?","options":["Research","Implementation"]}]});
        let history = turn(&json!({"id":"turn","items":[item]}));
        let event = crate::events::public_event(&json!({"method":"item/completed",
            "params":{"turnId":"turn","item":item}}))
        .ok_or("expected projected event")?;
        let SessionEvent::Message {
            delivery,
            questions,
            ..
        } = event
        else {
            return Err("expected a message".into());
        };
        assert_eq!(delivery.as_deref(), Some("async"));
        assert_eq!(questions.len(), 1);
        assert_eq!(questions[0].options, ["Research", "Implementation"]);
        assert_eq!(
            serde_json::to_value(questions)?,
            serde_json::to_value(&history.items[0].questions)?
        );
        assert!(super::questions(&json!({"questions":null})).is_empty());
        let free =
            super::questions(&json!({"questions":[{"title":"Your budget?","options":null}]}));
        assert_eq!(free.len(), 1);
        assert!(free[0].options.is_empty());
        Ok(())
    }
}
