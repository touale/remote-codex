use super::LocalStore;
use crate::{ClientError, Result};
use remote_codex_protocol::{Session, SessionBinding};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CachedSession {
    pub server_id: String,
    pub server: String,
    pub remote_identity: String,
    pub checked_at: i64,
    pub session: Session,
}

impl LocalStore {
    pub async fn save_session(&self, binding: &SessionBinding) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let updated = sqlx::query("INSERT INTO local_sessions(id,server,record) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET record=excluded.record,updated_at=unixepoch() WHERE local_sessions.server=excluded.server")
            .bind(&binding.session.id).bind(&binding.server_id).bind(serde_json::to_string(binding)?).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(ClientError::Argument(
                "session is already bound to another server",
            ));
        }
        tx.commit().await?;
        Ok(())
    }
    pub async fn session_binding(&self, id: &str) -> Result<SessionBinding> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT record FROM local_sessions WHERE id=?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(serde_json::from_str(&value.ok_or(ClientError::NotFound)?)?)
    }
    pub async fn cached_sessions(
        &self,
        server: Option<&str>,
        archived: bool,
    ) -> Result<Vec<CachedSession>> {
        let rows: Vec<(String,String,i64)> = sqlx::query_as("SELECT s.record,c.name,s.updated_at FROM local_sessions s JOIN connections c ON c.id=s.server WHERE (? IS NULL OR s.server=?) ORDER BY s.updated_at DESC,s.id")
            .bind(server).bind(server).fetch_all(&self.pool).await?;
        let mut result = Vec::new();
        for (record, server, checked_at) in rows {
            let binding: SessionBinding = serde_json::from_str(&record)?;
            if binding.session.archived != archived {
                continue;
            }
            result.push(CachedSession {
                server_id: binding.server_id,
                server,
                remote_identity: binding.remote_identity,
                checked_at,
                session: binding.session,
            });
        }
        Ok(result)
    }
}
