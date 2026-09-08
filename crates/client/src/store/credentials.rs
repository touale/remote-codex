use super::LocalStore;
use crate::{Result, config::SecretRef};

impl LocalStore {
    pub async fn installation_id(&self) -> Result<String> {
        Ok(sqlx::query_scalar("SELECT id FROM installation")
            .fetch_one(&self.pool)
            .await?)
    }

    pub(crate) async fn reserve_credential(&self, reference: &SecretRef) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("INSERT INTO credentials(id,state) VALUES (?,'pending')")
            .bind(reference.id())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn forget_pending_credential(&self, reference: &SecretRef) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("DELETE FROM credentials WHERE id=? AND state='pending'")
            .bind(reference.id())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
