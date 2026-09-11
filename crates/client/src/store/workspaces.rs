use super::LocalStore;
use crate::Result;
use remote_codex_core::workspace::Workspace;

impl LocalStore {
    pub(crate) async fn workspace_catalog(&self) -> Result<Vec<Workspace>> {
        let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT w.server,c.name,w.path,w.used_at FROM workspaces w JOIN connections c ON c.id=w.server ORDER BY c.name,w.used_at DESC,w.path"
        ).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|(server_id, server, path, used_at)| Workspace {
                server_id,
                server,
                path,
                used_at,
            })
            .collect())
    }
    pub(crate) async fn remove_workspace(&self, server: &str, path: &str) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query(
            "DELETE FROM local_sessions WHERE server=? AND json_extract(record,'$.session.cwd')=?",
        )
        .bind(server)
        .bind(path)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM project_trust WHERE server=? AND path=?")
            .bind(server)
            .bind(path)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM workspaces WHERE server=? AND path=?")
            .bind(server)
            .bind(path)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
