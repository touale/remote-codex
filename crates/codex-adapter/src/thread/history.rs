use super::{Codex, binding};
use remote_codex_core::session::{HistoryItem, HistoryPage, HistoryTurn, SessionBinding};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::path::Path;

pub async fn update(
    program: &Path,
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
    program: &Path,
    bound: &SessionBinding,
    cursor: Option<&str>,
) -> Result<HistoryPage, Fault> {
    let codex = Codex::start(program, Path::new(&bound.codex_home)).await?;
    let result = async {
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
    .await;
    codex.shutdown().await;
    result
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
                tool: crate::desktop::tool(item),
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
