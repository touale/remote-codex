use super::LocalStore;
use crate::{ClientError, Result};
use std::path::Path;

impl LocalStore {
    async fn access_write(&self, id: &str) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>> {
        let mut tx = self.begin_write().await?;
        let found: Option<String> = sqlx::query_scalar(
            "INSERT INTO server_access(server) SELECT id FROM connections WHERE id=?
             ON CONFLICT(server) DO UPDATE SET server=excluded.server RETURNING server",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        found.ok_or(ClientError::NotFound)?;
        Ok(tx)
    }

    /// Omitted options preserve the saved values; callers never write a read snapshot back.
    pub(crate) async fn set_ssh_options(
        &self,
        id: &str,
        identity: Option<&Path>,
        config: Option<&Path>,
    ) -> Result<()> {
        let mut tx = self.access_write(id).await?;
        sqlx::query(
            "UPDATE server_access SET identity_file=coalesce(?,identity_file),
             ssh_config=coalesce(?,ssh_config) WHERE server=?",
        )
        .bind(identity.map(|p| p.to_string_lossy().into_owned()))
        .bind(config.map(|p| p.to_string_lossy().into_owned()))
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn replace_identity(
        &self,
        id: &str,
        expected: Option<&Path>,
        next: Option<&Path>,
    ) -> Result<()> {
        let mut tx = self.access_write(id).await?;
        let count = sqlx::query(
            "UPDATE server_access SET identity_file=? WHERE server=? AND identity_file IS ?",
        )
        .bind(next.map(|p| p.to_string_lossy().into_owned()))
        .bind(id)
        .bind(expected.map(|p| p.to_string_lossy().into_owned()))
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if count != 1 {
            return Err(ClientError::RevisionConflict);
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn set_managed_key(&self, id: &str, path: &Path, public: &str) -> Result<()> {
        let mut tx = self.access_write(id).await?;
        sqlx::query("UPDATE server_access SET identity_file=?,managed_public_key=? WHERE server=?")
            .bind(path.to_string_lossy().into_owned())
            .bind(public)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn set_service_installation(
        &self,
        id: &str,
        program: &str,
        root: &str,
    ) -> Result<()> {
        let mut tx = self.access_write(id).await?;
        sqlx::query(
            "UPDATE server_access SET service_executable=?,service_root=coalesce(service_root,?)
             WHERE server=?",
        )
        .bind(program)
        .bind(root)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn acknowledge_config(
        &self,
        id: &str,
        revision: i64,
        identity: &str,
    ) -> Result<()> {
        let mut tx = self.access_write(id).await?;
        let stored: Option<String> =
            sqlx::query_scalar("SELECT remote_identity FROM server_access WHERE server=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if stored.as_deref().is_some_and(|old| old != identity) {
            return Err(remote_codex_protocol::Fault::new(
                "REMOTE_IDENTITY_CHANGED",
                "remote installation identity changed during synchronization",
            )
            .into());
        }
        sqlx::query(
            "UPDATE server_access SET applied_revision=?,remote_identity=?,health='ready',
             checked_at=unixepoch() WHERE server=? AND (applied_revision IS NULL OR applied_revision<=?)",
        )
        .bind(revision)
        .bind(identity)
        .bind(id)
        .bind(revision)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn mark_health(&self, id: &str, health: &str, checked_at: i64) -> Result<()> {
        let mut tx = self.access_write(id).await?;
        sqlx::query(
            "UPDATE server_access SET health=?,checked_at=? WHERE server=?
             AND (checked_at IS NULL OR checked_at<=?)",
        )
        .bind(health)
        .bind(checked_at)
        .bind(id)
        .bind(checked_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
