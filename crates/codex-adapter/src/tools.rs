//! Presentation data shared by live tool notifications and native history.
use remote_codex_core::desktop::{FileChange, ToolItem, ToolLink};
use serde_json::Value;

pub fn project(value: &Value) -> Option<ToolItem> {
    let kind = value["type"].as_str()?;
    let mut item = ToolItem {
        id: text(value, "id").into(),
        kind: kind.into(),
        title: String::new(),
        output: String::new(),
        output_source: None,
        status: text(value, "status").into(),
        changes: Vec::new(),
        input: None,
        links: Vec::new(),
    };
    let input = match kind {
        "commandExecution" => {
            item.title = text(value, "command").into();
            item.output = text(value, "aggregatedOutput").into();
            let mut input = item.title.clone();
            if let Some(cwd) = value["cwd"].as_str().filter(|s| !s.is_empty()) {
                input.push_str(&format!("\n\nDirectory: {cwd}"));
            }
            if let Some(code) = value["exitCode"].as_i64() {
                input.push_str(&format!("\nExit code: {code}"));
            }
            input
        }
        "fileChange" => {
            item.changes = array(&value["changes"])
                .map(|c| FileChange {
                    path: text(c, "path").into(),
                    diff: text(c, "diff").into(),
                })
                .collect();
            let count = item.changes.len();
            item.title = match count {
                0 => "File changes".into(),
                1 => "File changes · 1 file".into(),
                _ => format!("File changes · {count} files"),
            };
            String::new()
        }
        "reasoning" => {
            item.title = "Reasoning summary".into();
            item.output = content(&value["summary"]);
            String::new()
        }
        "mcpToolCall" | "dynamicToolCall" => {
            let owner = text(
                value,
                if kind == "mcpToolCall" {
                    "server"
                } else {
                    "namespace"
                },
            );
            item.title = joined([owner.into(), text(value, "tool").into()], " · ");
            item.output = if kind == "mcpToolCall" {
                joined(
                    [
                        content(&value["result"]["content"]),
                        formatted(&value["result"]["structuredContent"]),
                    ],
                    "\n\n",
                )
            } else {
                content(&value["contentItems"])
            };
            if !value["error"].is_null() {
                let error = value["error"]["message"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| formatted(&value["error"]));
                item.output = joined([item.output, format!("Error: {error}")], "\n\n");
            }
            formatted(&value["arguments"])
        }
        "webSearch" => web_search(value, &mut item),
        _ => return None,
    };
    item.input = (!input.trim().is_empty()).then_some(input);
    Some(item)
}

fn web_search(value: &Value, item: &mut ToolItem) -> String {
    let action = &value["action"];
    let mut queries: Vec<&str> = array(&action["queries"])
        .filter_map(Value::as_str)
        .filter(|q| !q.trim().is_empty())
        .collect();
    if queries.is_empty() {
        let query = action["query"]
            .as_str()
            .filter(|q| !q.trim().is_empty())
            .unwrap_or_else(|| text(value, "query"));
        if !query.trim().is_empty() {
            queries.push(query);
        }
    }
    let (label, target, input) = match text(action, "type") {
        "openPage" => ("Open webpage", text(action, "url"), String::new()),
        "findInPage" => (
            "Find in page",
            text(action, "pattern"),
            text(action, "pattern").into(),
        ),
        _ => (
            "Search the web",
            queries.first().copied().unwrap_or_default(),
            queries.join("\n"),
        ),
    };
    item.title = joined([label.into(), target.into()], " · ");
    if let Some(link) = link(text(action, "url"), text(action, "url"), None) {
        item.links.push(link);
    }
    let mut other = Vec::new();
    for result in array(&value["results"]) {
        let url = text(result, "url");
        let title = result["title"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(url);
        let description = result["snippet"]
            .as_str()
            .or_else(|| result["description"].as_str());
        if let Some(link) = link(url, title, description) {
            item.links.push(link);
        } else {
            other.push(formatted(result));
        }
    }
    item.output = joined(other, "\n\n");
    // An incomplete open/find action can still include a useful native query.
    if input.is_empty() && item.links.is_empty() {
        queries.join("\n")
    } else {
        input
    }
}

fn link(url: &str, title: &str, description: Option<&str>) -> Option<ToolLink> {
    let parsed = url::Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    Some(ToolLink {
        title: title.into(),
        url: url.into(),
        description: description.filter(|s| !s.is_empty()).map(str::to_owned),
    })
}

fn content(value: &Value) -> String {
    joined(
        array(value).map(|part| {
            if let Some(text) = part.as_str().or_else(|| part["text"].as_str()) {
                return text.into();
            }
            match text(part, "type") {
                "image" | "inputImage" => "Image result (preview unavailable).".into(),
                "audio" | "inputAudio" => "Audio result (preview unavailable).".into(),
                "resource" => part["resource"]["text"]
                    .as_str()
                    .unwrap_or("Resource result (preview unavailable).")
                    .into(),
                _ => formatted(part),
            }
        }),
        "\n\n",
    )
}

fn array(value: &Value) -> impl Iterator<Item = &Value> {
    value.as_array().into_iter().flatten()
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}
fn formatted(value: &Value) -> String {
    if value.is_null() {
        return String::new();
    }
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| serde_json::to_string_pretty(value).unwrap_or_default())
}
fn joined(parts: impl IntoIterator<Item = String>, separator: &str) -> String {
    parts
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(separator)
}

#[cfg(test)]
mod tests;
