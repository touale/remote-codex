use super::LocalStore;
use crate::Result;
use remote_codex_core::session::HistoryPage;

impl LocalStore {
    pub(crate) async fn message_time(&self, session: &str, client_id: &str) -> Result<Option<i64>> {
        Ok(
            sqlx::query_scalar("SELECT sent_at FROM message_times WHERE session=? AND client_id=?")
                .bind(session)
                .bind(client_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }
    pub(crate) async fn remember_message(
        &self,
        session: &str,
        client_id: &str,
        sent_at: i64,
    ) -> Result<()> {
        let mut tx = self.begin_write().await?;
        sqlx::query("INSERT OR IGNORE INTO message_times(session,client_id,sent_at) VALUES(?,?,?)")
            .bind(session)
            .bind(client_id)
            .bind(sent_at)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn message_times(&self, page: &mut HistoryPage) -> Result<()> {
        // Read only IDs in the requested native page; never load an unbounded session log.
        let ids: Vec<&str> = page
            .turns
            .iter()
            .flat_map(|t| &t.items)
            .filter_map(|item| item.client_id.as_deref())
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        let mut times = std::collections::HashMap::<String, i64>::new();
        for chunk in ids.chunks(200) {
            let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "SELECT client_id,sent_at FROM message_times WHERE session=",
            );
            query
                .push_bind(&page.session.id)
                .push(" AND client_id IN (");
            let mut values = query.separated(",");
            for id in chunk {
                values.push_bind(*id);
            }
            values.push_unseparated(")");
            times.extend(
                query
                    .build_query_as::<(String, i64)>()
                    .fetch_all(&self.pool)
                    .await?,
            );
        }
        for item in page.turns.iter_mut().flat_map(|t| &mut t.items) {
            item.sent_at = item
                .client_id
                .as_ref()
                .and_then(|id| times.get(id).copied());
        }
        Ok(())
    }
}
