use super::{Codex, SessionBinding, binding, turn};
use crate::program::Launch;
use remote_codex_core::{desktop::ToolOutputSource, session::HistoryPage};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::path::Path;

const ITEM_LIMIT: usize = 16;

pub(super) async fn page(
    codex: &Codex,
    bound: &SessionBinding,
    cursor: Option<&str>,
) -> Result<HistoryPage, Fault> {
    let mut position = match cursor {
        None => json!({"version":1,"thread":bound.session.id,"turns":null,"items":null}),
        Some(cursor) => {
            let value: Value = serde_json::from_str(cursor).map_err(|_| invalid())?;
            if value["version"] != 1 || value["thread"] != bound.session.id {
                return Err(invalid());
            }
            for key in ["turns", "items"] {
                if !value[key].is_null() && !value[key].is_string() {
                    return Err(invalid());
                }
            }
            if value["items"].is_string() && !value["turn"].is_object() {
                return Err(invalid());
            }
            value
        }
    };
    let summary = codex
        .engine
        .call(
            "thread/read",
            json!({"threadId":bound.session.id,"includeTurns":false}),
        )
        .await?;
    let mut page = HistoryPage {
        session: binding::session(&summary["thread"], &bound.session.cwd)?,
        turns: Vec::new(),
        next_cursor: None,
    };
    let mut count = 0;
    // Empty turns still have status/timing. Bound them independently of item count.
    while count < ITEM_LIMIT && page.turns.len() < 20 {
        let (metadata, next_turns) = if position["turn"].is_object() {
            (position["turn"].clone(), position["turns"].clone())
        } else {
            let turns = codex
                .engine
                .call(
                    "thread/turns/list",
                    json!({
                        "threadId":bound.session.id,"cursor":position["turns"],
                        "itemsView":"notLoaded","limit":1,"sortDirection":"desc"
                    }),
                )
                .await?;
            let Some(metadata) = data(&turns)?.first() else {
                page.next_cursor = None;
                break;
            };
            (metadata.clone(), turns["nextCursor"].clone())
        };
        let turn_id = metadata["id"].as_str().ok_or_else(invalid)?;
        let source_cursor = position["items"].as_str().map(str::to_owned);
        let mut items = codex
            .engine
            .call(
                "thread/items/list",
                json!({
                    "threadId":bound.session.id,"turnId":turn_id,"cursor":source_cursor,
                    "limit":ITEM_LIMIT-count,"sortDirection":"desc"
                }),
            )
            .await?;
        let entries = items["data"].as_array_mut().ok_or_else(invalid)?;
        let returned = entries.len();
        let mut raw = metadata.clone();
        let mut values = Vec::with_capacity(entries.len());
        for entry in entries.iter_mut().rev() {
            if entry["turnId"] != turn_id || !entry["item"].is_object() {
                return Err(invalid());
            }
            values.push(entry["item"].take());
        }
        raw["items"] = Value::Array(values);
        let mut projected = turn(&raw);
        projected.items_before = items["nextCursor"].is_string();
        for item in &mut projected.items {
            if let Some(tool) = &mut item.tool
                && !tool.output.is_empty()
                && tool.status != "inProgress"
            {
                tool.output = String::new();
                tool.output_source = Some(ToolOutputSource {
                    session: bound.session.id.clone(),
                    turn: turn_id.into(),
                    cursor: source_cursor.clone(),
                    item: item.id.clone(),
                });
            }
        }
        page.turns.push(projected);
        count += returned;
        if let Some(next) = items["nextCursor"].as_str() {
            if source_cursor.as_deref() == Some(next) {
                return Err(invalid());
            }
            // Pin the current turn while paging it, even if newer turns arrive.
            position["turn"] = metadata;
            position["turns"] = next_turns;
            position["items"] = json!(next);
        } else if let Some(next) = next_turns.as_str() {
            position["turns"] = json!(next);
            position["items"] = Value::Null;
            position["turn"] = Value::Null;
        } else {
            page.next_cursor = None;
            return Ok(page);
        }
        page.next_cursor = Some(position.to_string());
        // Return one fragment of a long turn; the next request continues it.
        if items["nextCursor"].is_string() {
            break;
        }
    }
    Ok(page)
}

pub async fn read_output(
    program: &Launch,
    bound: &SessionBinding,
    source: &ToolOutputSource,
) -> Result<String, Fault> {
    if source.session != bound.session.id {
        return Err(invalid());
    }
    let codex = Codex::start(program, Path::new(&bound.codex_home)).await?;
    let result = async {
        let mut cursor = source.cursor.clone();
        loop {
            let response = codex
                .engine
                .call(
                    "thread/items/list",
                    json!({
                        "threadId":bound.session.id,"turnId":source.turn,"cursor":cursor,
                        "limit":ITEM_LIMIT,"sortDirection":"desc"
                    }),
                )
                .await?;
            if let Some(tool) = data(&response)?
                .iter()
                .find(|entry| entry["turnId"] == source.turn && entry["item"]["id"] == source.item)
                .and_then(|entry| crate::desktop::tool(&entry["item"]))
            {
                return Ok(tool.output);
            }
            // The first page has no anchor; new items may have pushed this result
            // into an older page since its heading was displayed.
            let Some(next) = response["nextCursor"].as_str() else {
                return Err(Fault::new(
                    "HISTORY_ITEM_UNAVAILABLE",
                    "This tool result is no longer available. Reload the conversation.",
                ));
            };
            if cursor.as_deref() == Some(next) {
                return Err(invalid());
            }
            cursor = Some(next.into());
        }
    }
    .await;
    codex.shutdown().await;
    result
}

fn data(value: &Value) -> Result<&Vec<Value>, Fault> {
    value["data"].as_array().ok_or_else(invalid)
}

fn invalid() -> Fault {
    Fault::new(
        "INVALID_NATIVE_HISTORY",
        "History pagination is invalid or stale. Reload the conversation.",
    )
}
