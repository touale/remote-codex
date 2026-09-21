use crate::{
    ClientError, Result,
    protocol::{ExecutionCommand, Request},
    remote::Remote,
    store::LocalStore,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;

#[derive(Clone)]
pub(crate) struct ProjectMcp {
    pub(crate) path: String,
    pub(crate) names: Vec<String>,
    pub(crate) trusted: bool,
    digest: String,
    servers: BTreeMap<String, Value>,
}

#[derive(Default)]
pub(crate) struct McpPlan {
    pub(crate) config: BTreeMap<String, Value>,
    pub(crate) commands: Vec<ExecutionCommand>,
}

pub(crate) async fn inspect(store: &LocalStore, remote: &Remote, path: &str) -> Result<ProjectMcp> {
    let response = remote
        .call(Request::ProjectConfig { path: path.into() })
        .await?;
    let path = response["path"]
        .as_str()
        .ok_or(ClientError::RemoteResponse)?
        .to_owned();
    let document: toml::Value = toml::from_str(response["config"].as_str().unwrap_or(""))
        .map_err(|_| ClientError::Argument("remote project config contains invalid TOML"))?;
    let mut servers = BTreeMap::new();
    if let Some(table) = document.get("mcp_servers").and_then(toml::Value::as_table) {
        if table.len() > 32 {
            return Err(ClientError::Argument(
                "project config exceeds 32 MCP servers",
            ));
        }
        for (name, value) in table {
            if name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
            {
                return Err(ClientError::Argument("invalid project MCP server name"));
            }
            let value = serde_json::to_value(value)?;
            if value["enabled"] == false {
                continue;
            }
            validate(&value)?;
            servers.insert(name.clone(), value);
        }
    }
    let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&servers)?));
    let stored = store.project_trust(&remote.server.id, &path).await?;
    Ok(ProjectMcp {
        path,
        names: servers.keys().cloned().collect(),
        trusted: stored.as_deref() == Some(&digest),
        digest,
        servers,
    })
}

impl ProjectMcp {
    pub(crate) async fn trust(&mut self, store: &LocalStore, server: &str) -> Result<()> {
        store
            .trust_project(server, &self.path, &self.digest)
            .await?;
        self.trusted = true;
        Ok(())
    }

    pub(crate) fn resolve(
        &self,
        home: &Path,
        environment: &str,
        conflict_source: Option<&str>,
    ) -> Result<McpPlan> {
        if !self.names.is_empty() && !self.trusted {
            return Err(ClientError::Argument(
                "project MCP configuration requires trust; rerun with --trust-project-mcp after reviewing .codex/config.toml",
            ));
        }
        let local = match std::fs::read_to_string(home.join("config.toml")) {
            Ok(text) => toml::from_str::<toml::Value>(&text)
                .map_err(|_| invalid_local("local Codex config.toml contains invalid TOML"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                toml::Value::Table(Default::default())
            }
            Err(error) => {
                return Err(invalid_local(&format!(
                    "cannot read local Codex config.toml: {error}"
                )));
            }
        };
        if local
            .get("mcp_servers")
            .is_some_and(|value| !value.is_table())
        {
            return Err(invalid_local("local mcp_servers must be a table"));
        }
        let mut plan = McpPlan::default();
        if let Some(servers) = local.get("mcp_servers").and_then(toml::Value::as_table) {
            for (name, definition) in servers {
                if !definition.is_table() {
                    return Err(invalid_local("local MCP server settings must be a table"));
                }
                let mut settings = serde_json::to_value(definition)?;
                if settings["enabled"] == false {
                    plan.config.insert(format!("mcp_servers.{name}"), settings);
                    continue;
                }
                if settings["environment_id"]
                    .as_str()
                    .is_some_and(|id| id != "local")
                {
                    return Err(invalid_local(
                        "local Codex MCP references another execution environment; configure remote MCP in this project's .codex/config.toml",
                    ));
                }
                settings["environment_id"] = json!("local");
                if settings.get("command").is_some() && settings.get("cwd").is_none() {
                    settings["cwd"] = json!(home.parent().unwrap_or(home));
                }
                plan.config.insert(format!("mcp_servers.{name}"), settings);
            }
        }
        for (name, settings) in &self.servers {
            let conflict = local.get("mcp_servers").and_then(|v| v.get(name)).is_some();
            if conflict {
                match conflict_source {
                    Some("local") => continue,
                    Some("remote") => {}
                    _ => {
                        return Err(ClientError::McpNameConflict);
                    }
                }
            }
            let mut settings = settings.clone();
            settings["environment_id"] = json!(environment);
            settings["cwd"] = json!(self.path);
            settings["required"] = json!(true);
            let command = settings["command"]
                .as_str()
                .ok_or(ClientError::RemoteResponse)?;
            let mut argv = vec![command.into()];
            for arg in settings["args"].as_array().into_iter().flatten() {
                argv.push(
                    arg.as_str()
                        .ok_or(ClientError::Argument("MCP args must contain strings"))?
                        .into(),
                );
            }
            plan.commands.push(ExecutionCommand {
                argv,
                cwd: self.path.clone(),
            });
            plan.config.insert(format!("mcp_servers.{name}"), settings);
        }
        Ok(plan)
    }
}

pub(crate) fn validate(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or(ClientError::Argument("MCP server must be a table"))?;
    const ALLOWED: &[&str] = &[
        "command",
        "args",
        "env",
        "cwd",
        "enabled",
        "required",
        "startup_timeout_sec",
        "tool_timeout_sec",
        "enabled_tools",
        "disabled_tools",
    ];
    if object.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(ClientError::Argument(
            "remote project MCP supports explicit stdio command, args, env and tool settings only",
        ));
    }
    if value["command"]
        .as_str()
        .is_none_or(|cmd| cmd.is_empty() || cmd.chars().any(char::is_control))
    {
        return Err(ClientError::Argument("project MCP command is invalid"));
    }
    if let Some(env) = value["env"].as_object() {
        for (key, value) in env {
            crate::config::EnvironmentName::parse(key)?;
            if !value.is_string() {
                return Err(ClientError::Argument(
                    "MCP environment values must be strings",
                ));
            }
        }
    }
    Ok(())
}

fn invalid_local(message: &str) -> ClientError {
    remote_codex_protocol::Fault::new(
        "MCP_CONFIGURATION_INVALID",
        &format!("{message}; fix the configuration and retry in this session"),
    )
    .into()
}
