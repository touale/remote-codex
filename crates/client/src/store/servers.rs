use super::LocalStore;
use crate::{ClientError, Result};
use serde::Serialize;
use sqlx::Row;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ServerAccess {
    pub(crate) identity_file: Option<PathBuf>,
    #[serde(skip_serializing)]
    pub(crate) managed_public_key: Option<String>,
    pub(crate) ssh_config: Option<PathBuf>,
    pub(crate) remote_identity: Option<String>,
    pub(crate) service_executable: Option<String>,
    pub(crate) service_root: Option<String>,
    pub(crate) applied_revision: Option<i64>,
    pub(crate) checked_at: Option<i64>,
    pub(crate) health: String,
}

impl LocalStore {
    pub(crate) async fn server_access(&self, id: &str) -> Result<ServerAccess> {
        self.connection_by_id(id).await?;
        let row = sqlx::query("SELECT * FROM server_access WHERE server=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Ok(ServerAccess {
                health: "unknown".into(),
                ..Default::default()
            });
        };
        Ok(ServerAccess {
            identity_file: row
                .try_get::<Option<String>, _>("identity_file")?
                .map(PathBuf::from),
            managed_public_key: row.try_get("managed_public_key")?,
            ssh_config: row
                .try_get::<Option<String>, _>("ssh_config")?
                .map(PathBuf::from),
            remote_identity: row.try_get("remote_identity")?,
            service_executable: row.try_get("service_executable")?,
            service_root: row.try_get("service_root")?,
            applied_revision: row.try_get("applied_revision")?,
            checked_at: row.try_get("checked_at")?,
            health: row.try_get("health")?,
        })
    }

    pub(crate) async fn remember_workspace(&self, server: &str, path: &str) -> Result<()> {
        if !path.starts_with('/') || path.chars().any(char::is_control) {
            return Err(ClientError::Argument(
                "workspace must be a remote absolute path",
            ));
        }
        let mut tx = self.begin_write().await?;
        sqlx::query("INSERT INTO workspace_history(server,path) VALUES(?,?) ON CONFLICT(server,path) DO UPDATE SET used_at=unixepoch()")
            .bind(server).bind(path).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM workspace_history WHERE server=? AND path NOT IN (SELECT path FROM workspace_history WHERE server=? ORDER BY used_at DESC,path LIMIT 30)")
            .bind(server).bind(server).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn workspaces(&self, server: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT path FROM workspace_history WHERE server=? ORDER BY used_at DESC,path LIMIT 30",
        )
        .bind(server)
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn remove_server(&self, id: &str) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("UPDATE ssh_credentials SET state='retired' WHERE server=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM local_sessions WHERE server=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM connections WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM server_revisions WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE credentials SET state='retired' WHERE managed=1 AND state='active' AND NOT EXISTS(SELECT 1 FROM settings WHERE representation='secret' AND value=credentials.id)")
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
