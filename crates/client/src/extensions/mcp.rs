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

pub struct ProjectMcp {
    pub path: String,
    pub names: Vec<String>,
    pub trusted: bool,
    digest: String,
    servers: BTreeMap<String, Value>,
}

#[derive(Default)]
pub struct McpPlan {
    pub config: BTreeMap<String, Value>,
    pub commands: Vec<ExecutionCommand>,
}

pub async fn inspect(store: &LocalStore, remote: &Remote, path: &str) -> Result<ProjectMcp> {
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
    let stored: Option<String> =
        sqlx::query_scalar("SELECT digest FROM project_trust WHERE server=? AND path=?")
            .bind(&remote.server.id)
            .bind(&path)
            .fetch_optional(&store.pool)
            .await?;
    Ok(ProjectMcp {
        path,
        names: servers.keys().cloned().collect(),
        trusted: stored.as_deref() == Some(&digest),
        digest,
        servers,
    })
}

impl ProjectMcp {
    pub async fn trust(&mut self, store: &LocalStore, server: &str) -> Result<()> {
        sqlx::query("INSERT INTO project_trust(server,path,digest) VALUES(?,?,?) ON CONFLICT(server,path) DO UPDATE SET digest=excluded.digest")
            .bind(server).bind(&self.path).bind(&self.digest).execute(&store.pool).await?;
        self.trusted = true;
        Ok(())
    }

    pub fn resolve(
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
                .map_err(|_| ClientError::Argument("local Codex config contains invalid TOML"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                toml::Value::Table(Default::default())
            }
            Err(error) => return Err(error.into()),
        };
        let mut plan = McpPlan::default();
        if let Some(servers) = local.get("mcp_servers").and_then(toml::Value::as_table) {
            for (name, definition) in servers {
                let mut settings = serde_json::to_value(definition)?;
                if settings["enabled"] == false {
                    continue;
                }
                if settings["environment_id"]
                    .as_str()
                    .is_some_and(|id| id != "local")
                {
                    return Err(ClientError::Argument(
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
                        return Err(ClientError::Argument(
                            "local and project MCP names conflict; select --mcp-source local or --mcp-source remote",
                        ));
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

fn validate(value: &Value) -> Result<()> {
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
