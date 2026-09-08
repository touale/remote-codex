use super::{Codex, binding};
use remote_codex_core::session::{HistoryItem, HistoryPage, HistoryTurn, SessionBinding};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::path::Path;

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
                json!({"threadId":bound.session.id,"cursor":cursor}),
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
        items: value["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| HistoryItem {
                id: text(item, "id"),
                kind: text(item, "type"),
                text: item_text(item),
            })
            .collect(),
    }
}

fn item_text(item: &Value) -> String {
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
