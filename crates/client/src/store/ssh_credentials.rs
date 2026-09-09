use super::LocalStore;
use crate::{ClientError, Result, config::SecretRef};

impl LocalStore {
    pub(crate) async fn ssh_credential(&self, server: &str) -> Result<Option<SecretRef>> {
        self.connection_by_id(server).await?;
        let id: Option<String> =
            sqlx::query_scalar("SELECT id FROM ssh_credentials WHERE server=? AND state='active'")
                .bind(server)
                .fetch_optional(&self.pool)
                .await?;
        id.as_deref()
            .map(SecretRef::new)
            .transpose()
            .map_err(Into::into)
    }

    pub(crate) async fn reserve_ssh_credential(
        &self,
        server: &str,
        id: &SecretRef,
        target: &str,
    ) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let count = sqlx::query(
            "INSERT INTO ssh_credentials(id,server,target,state) SELECT ?,id,?,'pending' FROM connections WHERE id=?",
        ).bind(id.id()).bind(target).bind(server).execute(&mut *tx).await?.rows_affected();
        if count != 1 {
            return Err(ClientError::NotFound);
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn ssh_credential_target(
        &self,
        server: &str,
        reference: &SecretRef,
    ) -> Result<String> {
        sqlx::query_scalar("SELECT target FROM ssh_credentials WHERE server=? AND id=? AND state IN ('active','pending')")
            .bind(server).bind(reference.id()).fetch_optional(&self.pool).await?
            .ok_or(ClientError::NotFound)
    }

    /// Compare the previous reference so concurrent updates cannot overwrite each other.
    pub(crate) async fn activate_ssh_credential(
        &self,
        server: &str,
        expected: Option<&SecretRef>,
        next: &SecretRef,
    ) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let current: Option<String> =
            sqlx::query_scalar("SELECT id FROM ssh_credentials WHERE server=? AND state='active'")
                .bind(server)
                .fetch_optional(&mut *tx)
                .await?;
        if current.as_deref() != expected.map(SecretRef::id) {
            return Err(ClientError::RevisionConflict);
        }
        sqlx::query("UPDATE ssh_credentials SET state='retired' WHERE server=? AND state='active'")
            .bind(server)
            .execute(&mut *tx)
            .await?;
        let count = sqlx::query("UPDATE ssh_credentials SET state='active' WHERE id=? AND server=? AND state='pending' AND EXISTS(SELECT 1 FROM connections WHERE id=?)")
            .bind(next.id()).bind(server).bind(server).execute(&mut *tx).await?.rows_affected();
        if count != 1 {
            return Err(ClientError::RevisionConflict);
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn retire_ssh_credential(&self, id: &SecretRef) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("UPDATE ssh_credentials SET state='retired' WHERE id=? AND state='pending'")
            .bind(id.id())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn forget_ssh_credentials(&self, server: &str) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("UPDATE ssh_credentials SET state='retired' WHERE server=?")
            .bind(server)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
