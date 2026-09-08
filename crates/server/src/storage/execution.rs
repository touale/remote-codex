use super::Store;
use crate::{Checked, Result};
use remote_codex_protocol::Fault;
use serde_json::Value;

impl Store {
    pub(crate) async fn record_permissions(
        &self,
        permissions: &remote_codex_protocol::SessionPermissions,
    ) -> Result<()> {
        let written = sqlx::query("INSERT INTO session_permissions(id,channel,thread) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET id=id WHERE channel=excluded.channel AND thread=excluded.thread")
            .bind(&permissions.id).bind(&permissions.channel).bind(&permissions.thread)
            .execute(&self.pool).await.checked("PERMISSION_STORAGE", "cannot record session permissions")?;
        if written.rows_affected() != 1 {
            return Err(Fault::new(
                "APPROVAL_MISMATCH",
                "session permission identity reused in another context",
            ));
        }
        Ok(())
    }
    pub(crate) async fn record_approval(
        &self,
        approval: &remote_codex_protocol::CommandApproval,
    ) -> Result<()> {
        sqlx::query("INSERT INTO execution_approvals(id,channel,thread,turn,item,argv,cwd) VALUES(?,?,?,?,?,?,?)")
            .bind(&approval.id).bind(&approval.channel).bind(&approval.thread).bind(&approval.turn)
            .bind(&approval.item).bind(serde_json::to_string(&approval.argv).checked("PROTOCOL_ERROR", "cannot record approval")?).bind(&approval.cwd)
            .execute(&self.pool).await.checked("APPROVAL_REUSED", "approval has already authorized another operation")?;
        Ok(())
    }
    pub async fn create_channel(&self, id: &str, profile: &str, revision: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO execution_channels(id,profile,revision,state) VALUES(?,?,?,'running')",
        )
        .bind(id)
        .bind(profile)
        .bind(revision)
        .execute(&self.pool)
        .await
        .checked("EXECUTION_CONFLICT", "execution channel already exists")?;
        Ok(())
    }
    pub async fn channel_owner(&self, id: &str) -> Result<Option<String>> {
        sqlx::query_scalar("SELECT profile FROM execution_channels WHERE id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .checked("STORAGE_ERROR", "cannot read execution owner")
    }
    pub async fn mark_channel_lost(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE execution_channels SET state='lost' WHERE id=?")
            .bind(id)
            .execute(&self.pool)
            .await
            .checked("STORAGE_ERROR", "cannot close execution")?;
        sqlx::query("UPDATE jobs SET state='unknown',updated_at=unixepoch() WHERE channel=? AND state IN ('starting','running')").bind(id).execute(&self.pool).await.checked("STORAGE_ERROR","cannot reconcile execution")?;
        sqlx::query("DELETE FROM operations WHERE id LIKE ?")
            .bind(format!("{id}/%"))
            .execute(&self.pool)
            .await
            .checked("STORAGE_ERROR", "cannot retire closed requests")?;
        sqlx::query("DELETE FROM exec_events WHERE channel=?")
            .bind(id)
            .execute(&self.pool)
            .await
            .checked("STORAGE_ERROR", "cannot retire closed replay")?;
        Ok(())
    }
    pub async fn append_event(&self, channel: &str, value: &Value) -> Result<i64> {
        let encoded = serde_json::to_string(value)
            .checked("PROTOCOL_ERROR", "cannot encode executor response")?;
        self.append_record(channel, &encoded).await
    }
    pub async fn execution_events(&self, channel: &str, after: i64) -> Result<Vec<(i64, Value)>> {
        let floor: i64 =
            sqlx::query_scalar("SELECT replay_floor FROM execution_channels WHERE id=?")
                .bind(channel)
                .fetch_one(&self.pool)
                .await
                .checked("STORAGE_ERROR", "cannot inspect replay floor")?;
        if after < floor {
            return Err(Fault::unknown(
                "execution replay buffer expired; open a new local runtime and inspect retained jobs",
            ));
        }
        let rows:Vec<(i64,String)>=sqlx::query_as("SELECT cursor,record FROM exec_events WHERE channel=? AND cursor>? ORDER BY cursor LIMIT 64")
            .bind(channel).bind(after).fetch_all(&self.pool).await.checked("STORAGE_ERROR","cannot read execution responses")?;
        rows.into_iter()
            .map(|(cursor, record)| {
                Ok((
                    cursor,
                    serde_json::from_str(&record)
                        .checked("STORAGE_ERROR", "invalid execution response")?,
                ))
            })
            .collect()
    }
    pub async fn operation_state(&self, id: &str, digest: &str) -> Result<Option<Option<Value>>> {
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT digest,result FROM operations WHERE id=?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .checked("STORAGE_ERROR", "cannot inspect execution request")?;
        match row {
            None => Ok(None),
            Some((old, _)) if old != digest => Err(Fault::new(
                "REQUEST_CONFLICT",
                "request identity reused with different arguments",
            )),
            Some((_, value)) => Ok(Some(
                value
                    .map(|v| {
                        serde_json::from_str(&v)
                            .checked("STORAGE_ERROR", "invalid execution result")
                    })
                    .transpose()?,
            )),
        }
    }
    pub async fn link_operation(&self, id: &str, cursor: i64) -> Result<()> {
        sqlx::query("UPDATE operations SET event_cursor=? WHERE id=?")
            .bind(cursor)
            .bind(id)
            .execute(&self.pool)
            .await
            .checked("STORAGE_ERROR", "cannot link execution result")?;
        Ok(())
    }
    pub async fn acknowledge_events(&self, channel: &str, cursor: i64) -> Result<()> {
        self.prune_acknowledged(channel, cursor).await?;
        sqlx::query("UPDATE operations SET result='null' WHERE event_cursor<=? AND id LIKE ?")
            .bind(cursor)
            .bind(format!("{channel}/%"))
            .execute(&self.pool)
            .await
            .checked(
                "STORAGE_ERROR",
                "cannot compact acknowledged execution results",
            )?;
        Ok(())
    }
}
