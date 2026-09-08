use super::LocalStore;
use crate::{ClientError, Result};
use serde::Serialize;
use sqlx::Row;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ServerAccess {
    pub identity_file: Option<PathBuf>,
    #[serde(skip_serializing)]
    pub managed_public_key: Option<String>,
    pub ssh_config: Option<PathBuf>,
    pub remote_identity: Option<String>,
    pub service_executable: Option<String>,
    pub service_root: Option<String>,
    pub applied_revision: Option<i64>,
    pub checked_at: Option<i64>,
    pub health: String,
}

impl LocalStore {
    pub async fn server_access(&self, id: &str) -> Result<ServerAccess> {
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

    pub async fn save_access(&self, id: &str, access: &ServerAccess) -> Result<()> {
        let mut tx = self.begin_write().await?;
        if access.applied_revision.is_some() && access.service_executable.is_some() {
            sqlx::query("UPDATE connections SET phase='service_prepared' WHERE id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("INSERT INTO server_access(server,identity_file,managed_public_key,ssh_config,remote_identity,service_executable,service_root,applied_revision,checked_at,health) VALUES(?,?,?,?,?,?,?,?,?,?) ON CONFLICT(server) DO UPDATE SET identity_file=excluded.identity_file,managed_public_key=excluded.managed_public_key,ssh_config=excluded.ssh_config,remote_identity=excluded.remote_identity,service_executable=excluded.service_executable,service_root=excluded.service_root,applied_revision=excluded.applied_revision,checked_at=excluded.checked_at,health=excluded.health")
            .bind(id).bind(access.identity_file.as_ref().map(|p|p.to_string_lossy().into_owned())).bind(&access.managed_public_key)
            .bind(access.ssh_config.as_ref().map(|p|p.to_string_lossy().into_owned())).bind(&access.remote_identity)
            .bind(&access.service_executable).bind(&access.service_root).bind(access.applied_revision).bind(access.checked_at).bind(&access.health)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn remember_workspace(&self, server: &str, path: &str) -> Result<()> {
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

    pub async fn workspaces(&self, server: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT path FROM workspace_history WHERE server=? ORDER BY used_at DESC,path LIMIT 30",
        )
        .bind(server)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn remove_server(&self, id: &str) -> Result<()> {
        let mut tx = self.begin_write().await?;
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
        sqlx::query("UPDATE credentials SET state='retired' WHERE state='active' AND NOT EXISTS (SELECT 1 FROM retained_legacy_credentials r WHERE r.id=credentials.id) AND NOT EXISTS(SELECT 1 FROM settings WHERE representation='secret' AND value=credentials.id)")
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
