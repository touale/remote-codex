//! Native configuration preview without starting a thread or opening an environment.
use crate::engine::Engine;
use remote_codex_core::desktop::{ConfirmedSettings, ModelOption, SessionDefaults};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};

pub(crate) struct Catalog {
    pub models: Vec<ModelOption>,
    default_model: Option<String>,
}
pub(crate) async fn read(engine: &Engine, mode: &str) -> Result<SessionDefaults, Fault> {
    let (config, catalog) = tokio::try_join!(
        engine.call("config/read", json!({"includeLayers":false})),
        catalog(engine)
    )?;
    resolve(&config["config"], catalog, mode)
}
fn resolve(config: &Value, catalog: Catalog, mode: &str) -> Result<SessionDefaults, Fault> {
    let model = config["model"]
        .as_str()
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .or(catalog.default_model)
        .ok_or_else(|| {
            Fault::new(
                "NATIVE_DEFAULTS_UNAVAILABLE",
                "Codex did not report a default model.",
            )
        })?;
    let effort = config["model_reasoning_effort"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            catalog
                .models
                .iter()
                .find(|m| m.id == model)
                .map(|m| m.default_effort.clone())
                .filter(|v| !v.is_empty())
        });
    Ok(SessionDefaults {
        settings: ConfirmedSettings {
            mode: remote_codex_core::goals::CollaborationMode::Agent,
            model,
            effort,
            full_access: crate::thread::binding::full_access(mode),
            // New threads start on-request, regardless of their execution ceiling.
            approval_policy: crate::thread::binding::START_APPROVAL_POLICY.into(),
            reviewer: config["approvals_reviewer"]
                .as_str()
                .unwrap_or("user")
                .into(),
        },
        models: catalog.models,
    })
}
pub(crate) async fn catalog(engine: &Engine) -> Result<Catalog, Fault> {
    let mut result = Vec::new();
    let mut default_model = None;
    let mut cursor = None;
    for _ in 0..10 {
        let value = engine
            .call("model/list", json!({"cursor":cursor,"limit":100}))
            .await?;
        for model in value["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| m["hidden"] != true)
        {
            if model["isDefault"] == true {
                default_model = model["model"].as_str().map(str::to_owned);
            }
            result.push(ModelOption {
                id: text(model, "model"),
                name: text(model, "displayName"),
                description: text(model, "description"),
                default_effort: text(model, "defaultReasoningEffort"),
                efforts: model["supportedReasoningEfforts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e["reasoningEffort"].as_str().map(str::to_owned))
                    .collect(),
            });
        }
        cursor = value["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            return Ok(Catalog {
                models: result,
                default_model,
            });
        }
    }
    Err(Fault::new(
        "NATIVE_CATALOG_LIMIT",
        "Model catalog exceeded its page limit.",
    ))
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog() -> Catalog {
        Catalog {
            default_model: Some("native-default".into()),
            models: vec![ModelOption {
                id: "native-default".into(),
                name: "Native".into(),
                description: String::new(),
                efforts: vec!["medium".into()],
                default_effort: "medium".into(),
            }],
        }
    }
    #[test]
    fn native_defaults_and_server_policy_are_independent() -> Result<(), Fault> {
        let value = resolve(
            &json!({"sandbox_mode":"danger-full-access"}),
            catalog(),
            "sandboxed",
        )?;
        assert_eq!(value.settings.model, "native-default");
        assert_eq!(value.settings.effort.as_deref(), Some("medium"));
        assert_eq!(value.settings.reviewer, "user");
        assert!(!value.settings.full_access);
        assert_eq!(value.settings.approval_policy, "on-request");
        let value = resolve(
            &json!({"model":"custom", "model_reasoning_effort":"high", "approvals_reviewer":"auto_review"}),
            catalog(),
            "unrestricted",
        )?;
        assert_eq!(value.settings.model, "custom");
        assert_eq!(value.settings.effort.as_deref(), Some("high"));
        assert_eq!(value.settings.reviewer, "auto_review");
        assert!(value.settings.full_access);
        assert_eq!(value.settings.approval_policy, "on-request");
        Ok(())
    }
    #[test]
    fn missing_native_default_is_an_error_not_first_catalog_entry() {
        let mut models = catalog();
        models.default_model = None;
        assert_eq!(
            resolve(&json!({}), models, "sandboxed")
                .err()
                .map(|error| error.code),
            Some("NATIVE_DEFAULTS_UNAVAILABLE".into())
        );
    }
}
