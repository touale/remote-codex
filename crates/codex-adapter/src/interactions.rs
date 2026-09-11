use remote_codex_core::desktop::{InputField, Interaction, InteractionAnswer, InteractionDecision};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};

pub fn project(event: &Value) -> Interaction {
    let params = &event["params"];
    if event["method"] == "item/tool/requestUserInput" {
        let fields = params["questions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|q| InputField {
                id: text(q, "id"),
                label: text(q, "question"),
                description: text(q, "header"),
                kind: "string".into(),
                required: true,
                secret: q["isSecret"] == true,
                choices: q["options"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|o| o["label"].as_str().map(str::to_owned))
                    .collect(),
            })
            .collect();
        return Interaction::Questions { fields };
    }
    if event["method"] == "mcpServer/elicitation/request" {
        let server = text(params, "serverName");
        let message = text(params, "message");
        if params["mode"] == "url" {
            return Interaction::McpUrl {
                server,
                message,
                url: text(params, "url"),
            };
        }
        if params["mode"] == "form" {
            let schema = &params["requestedSchema"];
            if let Some(properties) = schema["properties"].as_object() {
                let mut fields = Vec::new();
                for (id, value) in properties {
                    let kind = text(value, "type");
                    if !matches!(
                        kind.as_str(),
                        "string" | "boolean" | "number" | "integer" | "array"
                    ) {
                        return unsupported();
                    }
                    let choices = choices(value);
                    if kind == "array" && choices.is_empty() {
                        return unsupported();
                    }
                    fields.push(InputField {
                        id: id.clone(),
                        label: value["title"].as_str().unwrap_or(id).into(),
                        description: text(value, "description"),
                        kind,
                        secret: false,
                        required: schema["required"]
                            .as_array()
                            .is_some_and(|v| v.contains(&json!(id))),
                        choices,
                    });
                }
                return Interaction::McpForm {
                    server,
                    message,
                    fields,
                };
            }
        }
    }
    unsupported()
}

pub fn answer(event: &Value, answer: InteractionAnswer) -> Result<Value, Fault> {
    let action = match answer.action {
        InteractionDecision::Accept => "accept",
        InteractionDecision::Decline => "decline",
        InteractionDecision::Cancel => "cancel",
    };
    let result = match project(event) {
        Interaction::Questions { fields } => {
            let mut answers = serde_json::Map::new();
            for field in fields {
                let values = if action == "accept" {
                    vec![answer.values.get(&field.id).cloned().ok_or_else(invalid)?]
                } else {
                    Vec::new()
                };
                answers.insert(field.id, json!({"answers":values}));
            }
            json!({"answers":answers})
        }
        Interaction::McpForm { fields, .. } => {
            let mut content = serde_json::Map::new();
            if action == "accept" {
                for field in fields {
                    let Some(value) = answer.values.get(&field.id).filter(|v| !v.is_empty()) else {
                        if field.required {
                            return Err(invalid());
                        } else {
                            continue;
                        }
                    };
                    let schema = &event["params"]["requestedSchema"]["properties"][&field.id];
                    let parsed = match field.kind.as_str() {
                        "string" => json!(value),
                        _ => serde_json::from_str::<Value>(value).map_err(|_| invalid())?,
                    };
                    validate(&parsed, schema, &field)?;
                    content.insert(field.id, parsed);
                }
            }
            json!({"action":action,"content": if action == "accept" {Some(content)} else {None}})
        }
        Interaction::McpUrl { .. } => json!({"action":action}),
        Interaction::Unsupported { .. } => {
            if action == "accept" {
                return Err(invalid());
            }
            return Ok(
                json!({"id":event["id"],"error":{"code":-32601,"message":"This interaction is not supported by Remote Codex Desktop."}}),
            );
        }
    };
    Ok(json!({"id":event["id"],"result":result}))
}

fn validate(value: &Value, schema: &Value, field: &InputField) -> Result<(), Fault> {
    let valid = match field.kind.as_str() {
        "string" => value.as_str().is_some_and(|v| {
            let count = v.chars().count() as u64;
            schema["minLength"].as_u64().is_none_or(|min| count >= min)
                && schema["maxLength"].as_u64().is_none_or(|max| count <= max)
                && (field.choices.is_empty() || field.choices.iter().any(|choice| choice == v))
        }),
        "boolean" => value.is_boolean(),
        "number" | "integer" => value.as_f64().is_some_and(|v| {
            (field.kind != "integer" || v.fract() == 0.0)
                && schema["minimum"].as_f64().is_none_or(|min| v >= min)
                && schema["maximum"].as_f64().is_none_or(|max| v <= max)
        }),
        "array" => value.as_array().is_some_and(|values| {
            values.iter().all(|v| {
                v.as_str()
                    .is_some_and(|v| field.choices.iter().any(|choice| choice == v))
            }) && schema["minItems"]
                .as_u64()
                .is_none_or(|min| values.len() as u64 >= min)
                && schema["maxItems"]
                    .as_u64()
                    .is_none_or(|max| values.len() as u64 <= max)
        }),
        _ => false,
    };
    if valid { Ok(()) } else { Err(invalid()) }
}
fn choices(value: &Value) -> Vec<String> {
    let value = if value["type"] == "array" {
        &value["items"]
    } else {
        value
    };
    if let Some(values) = value["enum"].as_array() {
        return values
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
    }
    value["oneOf"]
        .as_array()
        .or_else(|| value["anyOf"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v["const"].as_str().map(str::to_owned))
        .collect()
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().into()
}
fn invalid() -> Fault {
    Fault::new(
        "INVALID_INTERACTION_RESPONSE",
        "Complete the requested fields with valid values.",
    )
}
fn unsupported() -> Interaction {
    Interaction::Unsupported {
        message: "This native interaction is not supported yet. Cancel it to continue safely."
            .into(),
    }
}
