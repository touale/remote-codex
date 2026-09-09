use super::Store;
use crate::{Checked, Result};

impl Store {
    /// Keep replay/operation evidence for the most recent closed channels.
    /// Job history has its own output bounds and remains available after this.
    pub(super) async fn retire_closed_evidence(&self) -> Result<()> {
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .checked("STORAGE_ERROR", "cannot retire recovery evidence")?;
        let channels: Vec<String> = sqlx::query_scalar("SELECT id FROM execution_channels WHERE state='lost' AND coalesce(lost_reason,'')<>'evidence_expired' ORDER BY created_at DESC,rowid DESC LIMIT -1 OFFSET 32")
            .fetch_all(&mut *tx).await.checked("STORAGE_ERROR", "cannot inspect closed executions")?;
        for channel in channels {
            sqlx::query("DELETE FROM operations WHERE substr(id,1,instr(id,'/')-1)=?")
                .bind(&channel)
                .execute(&mut *tx)
                .await
                .checked("STORAGE_ERROR", "cannot retire closed operation evidence")?;
            for table in ["exec_events", "execution_approvals", "session_permissions"] {
                sqlx::query(&format!("DELETE FROM {table} WHERE channel=?"))
                    .bind(&channel)
                    .execute(&mut *tx)
                    .await
                    .checked("STORAGE_ERROR", "cannot retire closed execution evidence")?;
            }
            sqlx::query("UPDATE execution_channels SET lost_reason='evidence_expired',replay_bytes=0 WHERE id=?").bind(channel).execute(&mut *tx).await.checked("STORAGE_ERROR", "cannot record expired recovery evidence")?;
        }
        tx.commit()
            .await
            .checked("STORAGE_ERROR", "cannot commit recovery retention")
    }

    /// Bounded replay is separate from retained job output. Crossing the replay
    /// floor makes a stale attachment fail explicitly; it never silently skips.
    pub(super) async fn append_record(&self, channel: &str, record: &str) -> Result<i64> {
        let added = record.len() as i64;
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .checked("STORAGE_ERROR", "cannot account replay storage")?;
        let event = sqlx::query("INSERT INTO exec_events(channel,record) VALUES(?,?)")
            .bind(channel)
            .bind(record)
            .execute(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot persist execution response")?
            .last_insert_rowid();
        let mut bytes:i64=sqlx::query_scalar("UPDATE execution_channels SET replay_bytes=replay_bytes+? WHERE id=? RETURNING replay_bytes")
            .bind(added).bind(channel).fetch_one(&mut *tx).await.checked("STORAGE_ERROR","cannot account replay storage")?;
        let mut floor = 0;
        while bytes > 8 * 1024 * 1024 {
            let row:Option<(i64,i64)>=sqlx::query_as("SELECT cursor,length(CAST(record AS BLOB)) FROM exec_events WHERE channel=? ORDER BY cursor LIMIT 1")
                .bind(channel).fetch_optional(&mut *tx).await.checked("STORAGE_ERROR","cannot inspect replay buffer")?;
            let Some((cursor, size)) = row else {
                break;
            };
            sqlx::query("DELETE FROM exec_events WHERE cursor=?")
                .bind(cursor)
                .execute(&mut *tx)
                .await
                .checked("STORAGE_ERROR", "cannot trim replay buffer")?;
            bytes -= size;
            floor = cursor;
        }
        sqlx::query("UPDATE execution_channels SET replay_bytes=?,replay_floor=max(replay_floor,?) WHERE id=?")
            .bind(bytes).bind(floor).bind(channel).execute(&mut *tx).await.checked("STORAGE_ERROR","cannot update replay bounds")?;
        tx.commit()
            .await
            .checked("STORAGE_ERROR", "cannot commit replay bounds")?;
        Ok(event)
    }

    pub(super) async fn prune_acknowledged(&self, channel: &str, ack: i64) -> Result<()> {
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .checked("STORAGE_ERROR", "cannot retire replay storage")?;
        let bounds:(i64,i64)=sqlx::query_as("SELECT coalesce(sum(length(CAST(record AS BLOB))),0),coalesce(max(cursor),0) FROM exec_events WHERE channel=? AND cursor<=? AND cursor NOT IN (SELECT cursor FROM exec_events WHERE channel=? ORDER BY cursor DESC LIMIT 128)")
            .bind(channel).bind(ack).bind(channel).fetch_one(&mut *tx).await.checked("STORAGE_ERROR","cannot inspect acknowledged events")?;
        sqlx::query("DELETE FROM exec_events WHERE channel=? AND cursor<=?")
            .bind(channel)
            .bind(bounds.1)
            .execute(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot retire acknowledged events")?;
        sqlx::query("UPDATE execution_channels SET replay_bytes=max(0,replay_bytes-?),replay_floor=max(replay_floor,?) WHERE id=?")
            .bind(bounds.0).bind(bounds.1).bind(channel).execute(&mut *tx).await.checked("STORAGE_ERROR","cannot update replay bounds")?;
        tx.commit()
            .await
            .checked("STORAGE_ERROR", "cannot commit replay cleanup")
    }
}
