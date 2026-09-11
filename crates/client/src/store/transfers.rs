use super::LocalStore;
use crate::{
    ClientError, Result,
    transfers::model::{Item, Task},
};
impl LocalStore {
    pub(crate) async fn transfer_remove_finished(&self, ids: &[String]) -> Result<Vec<String>> {
        let mut tx = self.begin_write().await?;
        let mut removed = Vec::new();
        for id in ids {
            if let Some(id) = sqlx::query_scalar::<_, String>("DELETE FROM transfer_tasks WHERE id=? AND json_extract(record,'$.view.status') IN ('completed','cancelled') RETURNING id")
                .bind(id).fetch_optional(&mut *tx).await? {
                removed.push(id);
            }
        }
        tx.commit().await?;
        Ok(removed)
    }

    pub(crate) async fn transfer_skipped(
        &self,
        id: &str,
        after: i64,
    ) -> Result<Vec<(i64, String)>> {
        Ok(sqlx::query_as("SELECT ordinal,json_extract(record,'$.source') FROM transfer_items WHERE task=? AND ordinal>? AND json_extract(record,'$.skipped')=1 ORDER BY ordinal LIMIT 101")
            .bind(id).bind(after).fetch_all(&self.pool).await?)
    }

    pub(crate) async fn transfer_progress(&self, id: &str) -> Result<(u64, usize, usize)> {
        let row: (i64, i64, i64) = sqlx::query_as("SELECT coalesce(sum(json_extract(record,'$.bytes')),0), coalesce(sum(json_extract(record,'$.done') AND NOT json_extract(record,'$.skipped')),0), coalesce(sum(json_extract(record,'$.skipped')),0) FROM transfer_items WHERE task=?")
            .bind(id).fetch_one(&self.pool).await?;
        Ok((row.0 as u64, row.1 as usize, row.2 as usize))
    }

    pub(crate) async fn transfer_save(&self, task: &Task) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("INSERT INTO transfer_tasks(id,server,record) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET record=excluded.record,updated_at=unixepoch()")
            .bind(&task.view.id).bind(&task.server_id).bind(serde_json::to_string(task)?)
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn transfer(&self, id: &str) -> Result<Task> {
        let record: String = sqlx::query_scalar("SELECT record FROM transfer_tasks WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(ClientError::NotFound)?;
        Ok(serde_json::from_str(&record)?)
    }
    pub(crate) async fn transfers(&self) -> Result<Vec<Task>> {
        let records: Vec<String> = sqlx::query_scalar(
            "SELECT record FROM transfer_tasks ORDER BY updated_at DESC,id DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        records
            .into_iter()
            .map(|r| serde_json::from_str(&r).map_err(Into::into))
            .collect()
    }
    pub(crate) async fn transfer_items(&self, id: &str) -> Result<Vec<Item>> {
        let records: Vec<String> =
            sqlx::query_scalar("SELECT record FROM transfer_items WHERE task=? ORDER BY ordinal")
                .bind(id)
                .fetch_all(&self.pool)
                .await?;
        records
            .into_iter()
            .map(|r| serde_json::from_str(&r).map_err(Into::into))
            .collect()
    }
    pub(crate) async fn transfer_item(&self, id: &str, item: &Item) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("INSERT INTO transfer_items(task,ordinal,record) VALUES(?,?,?) ON CONFLICT(task,ordinal) DO UPDATE SET record=excluded.record")
            .bind(id).bind(item.index).bind(serde_json::to_string(item)?).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn transfer_reset_scan(&self, id: &str) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("DELETE FROM transfer_items WHERE task=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
