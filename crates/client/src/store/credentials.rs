use super::LocalStore;
use crate::{Result, config::SecretRef};

pub(crate) enum RetiredCredential {
    Environment(SecretRef),
    Ssh(SecretRef),
}

impl RetiredCredential {
    pub(crate) fn reference(&self) -> &SecretRef {
        match self {
            Self::Environment(reference) | Self::Ssh(reference) => reference,
        }
    }
}

impl LocalStore {
    pub(crate) async fn has_credential_cleanup(&self) -> Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM credentials WHERE managed=1
             AND (state='retired' OR (state='pending' AND created_at<unixepoch()-86400))
             AND NOT EXISTS (SELECT 1 FROM settings WHERE representation='secret' AND value=credentials.id))
             OR EXISTS(SELECT 1 FROM ssh_credentials
             WHERE state='retired' OR (state='pending' AND created_at<unixepoch()-86400))",
        ).fetch_one(&self.pool).await?)
    }

    pub(crate) async fn installation_id(&self) -> Result<String> {
        Ok(sqlx::query_scalar("SELECT id FROM installation")
            .fetch_one(&self.pool)
            .await?)
    }

    pub(crate) async fn reserve_credential(&self, reference: &SecretRef) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("INSERT INTO credentials(id,state,managed) VALUES (?,'pending',1)")
            .bind(reference.id())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn retire_credential(&self, reference: &SecretRef) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query(
            "UPDATE credentials SET state='retired' WHERE id=? AND managed=1 AND state='pending'",
        )
        .bind(reference.id())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Called only while holding the exclusive vault-maintenance lock.
    pub(crate) async fn retired_credentials(&self) -> Result<Vec<RetiredCredential>> {
        let mut tx = self.begin_write().await?;
        sqlx::query("UPDATE credentials SET state='retired' WHERE managed=1 AND state='pending' AND created_at<unixepoch()-86400")
            .execute(&mut *tx).await?;
        sqlx::query("UPDATE ssh_credentials SET state='retired' WHERE state='pending' AND created_at<unixepoch()-86400")
            .execute(&mut *tx).await?;
        let environment: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM credentials WHERE managed=1 AND state='retired'
             AND NOT EXISTS (SELECT 1 FROM settings WHERE representation='secret' AND value=credentials.id)",
        ).fetch_all(&mut *tx).await?;
        let ssh: Vec<String> =
            sqlx::query_scalar("SELECT id FROM ssh_credentials WHERE state='retired'")
                .fetch_all(&mut *tx)
                .await?;
        tx.commit().await?;
        environment
            .iter()
            .map(|id| SecretRef::new(id).map(RetiredCredential::Environment))
            .chain(
                ssh.iter()
                    .map(|id| SecretRef::new(id).map(RetiredCredential::Ssh)),
            )
            .collect::<std::result::Result<_, _>>()
            .map_err(Into::into)
    }

    pub(crate) async fn forget_retired_credential(
        &self,
        credential: &RetiredCredential,
    ) -> Result<()> {
        let sql = match credential {
            RetiredCredential::Environment(_) => {
                "DELETE FROM credentials WHERE id=? AND managed=1 AND state='retired'"
            }
            RetiredCredential::Ssh(_) => {
                "DELETE FROM ssh_credentials WHERE id=? AND state='retired'"
            }
        };
        let mut tx = self.begin_write().await?;
        sqlx::query(sql)
            .bind(credential.reference().id())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
