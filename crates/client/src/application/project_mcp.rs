use super::WorkspaceHandle;
use crate::{ClientError, Result};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectMcpServer {
    pub name: String,
    pub definition: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct ProjectMcpConfig {
    pub servers: Vec<ProjectMcpServer>,
    pub revision: Option<String>,
}

impl WorkspaceHandle {
    pub async fn project_mcp(&self) -> Result<ProjectMcpConfig> {
        let (document, revision) = self.project_document().await?;
        let servers = document
            .get("mcp_servers")
            .and_then(Item::as_table)
            .map(|table| {
                table
                    .iter()
                    .map(|(name, item)| ProjectMcpServer {
                        name: name.into(),
                        definition: item
                            .as_table()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| item.to_string()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ProjectMcpConfig { servers, revision })
    }
    pub async fn save_project_mcp(
        &self,
        servers: Vec<ProjectMcpServer>,
        expected: Option<String>,
    ) -> Result<ProjectMcpConfig> {
        if servers.len() > 32 {
            return Err(ClientError::Argument(
                "A project supports up to 32 MCP servers.",
            ));
        }
        let (mut document, revision) = self.project_document().await?;
        if revision != expected {
            return Err(ClientError::RevisionConflict);
        }
        let mut updated = Table::new();
        for server in servers {
            if server.name.is_empty()
                || server.name.len() > 64
                || !server
                    .name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
                || updated.contains_key(&server.name)
            {
                return Err(ClientError::Argument(
                    "MCP names must be unique and contain letters, numbers, underscores or hyphens.",
                ));
            }
            let definition: DocumentMut = server
                .definition
                .parse()
                .map_err(|_| ClientError::Argument("MCP configuration contains invalid TOML."))?;
            let value: toml::Value = toml::from_str(&server.definition)
                .map_err(|_| ClientError::Argument("MCP configuration contains invalid TOML."))?;
            crate::extensions::mcp::validate(&serde_json::to_value(value)?)?;
            updated.insert(&server.name, Item::Table(definition.as_table().clone()));
        }
        document["mcp_servers"] = Item::Table(updated);
        if !self
            .files()
            .list("")
            .await?
            .entries
            .iter()
            .any(|entry| entry.name == ".codex")
        {
            self.files().create_directory(".codex").await?;
        }
        self.files()
            .write(".codex/config.toml", &document.to_string(), revision)
            .await?;
        self.project_mcp().await
    }
    async fn project_document(&self) -> Result<(DocumentMut, Option<String>)> {
        let root = self.files().list("").await?;
        if !root.entries.iter().any(|entry| entry.name == ".codex") {
            return Ok((DocumentMut::new(), None));
        }
        let directory = self.files().list(".codex").await?;
        if !directory
            .entries
            .iter()
            .any(|entry| entry.name == "config.toml")
        {
            return Ok((DocumentMut::new(), None));
        }
        let file = self.files().read(".codex/config.toml").await?;
        Ok((
            file.text.parse().map_err(|_| {
                ClientError::Argument("Remote project configuration contains invalid TOML.")
            })?,
            Some(file.revision),
        ))
    }
}
