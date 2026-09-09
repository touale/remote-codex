use super::LocalStore;
use crate::Result;

impl LocalStore {
    pub(crate) async fn project_trust(&self, server: &str, path: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT digest FROM project_trust WHERE server=? AND path=?")
                .bind(server)
                .bind(path)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub(crate) async fn trust_project(&self, server: &str, path: &str, digest: &str) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query(
            "INSERT INTO project_trust(server,path,digest) VALUES(?,?,?)
             ON CONFLICT(server,path) DO UPDATE SET digest=excluded.digest",
        )
        .bind(server)
        .bind(path)
        .bind(digest)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
