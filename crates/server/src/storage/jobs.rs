use super::Store;
use crate::{Checked, Result};
use remote_codex_protocol::{Fault, Job};
use serde_json::{Value, json};

type JobRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    Option<i64>,
    i64,
    i64,
    bool,
);
fn decode(
    (
        id,
        process_id,
        kind,
        channel,
        thread,
        cwd,
        state,
        exit_code,
        created_at,
        updated_at,
        output_truncated,
    ): JobRow,
) -> Job {
    Job {
        id,
        process_id,
        kind,
        channel,
        thread,
        cwd,
        state,
        exit_code,
        created_at,
        updated_at,
        output_truncated,
    }
}

impl Store {
    pub async fn running_processes(&self, channel: &str, mcp_only: bool) -> Result<Vec<String>> {
        sqlx::query_scalar("SELECT process_id FROM jobs WHERE channel=? AND state IN ('starting','running') AND (?=0 OR kind='mcp')")
            .bind(channel).bind(mcp_only).fetch_all(&self.pool).await.checked("STORAGE_ERROR","cannot inspect running processes")
    }
    pub async fn active_jobs(&self, channel: &str) -> Result<i64> {
        sqlx::query_scalar(
            "SELECT count(*) FROM jobs WHERE channel=? AND kind='command' AND state IN ('starting','running')",
        )
        .bind(channel)
        .fetch_one(&self.pool)
        .await
        .checked("STORAGE_ERROR", "cannot inspect active jobs")
    }
    pub async fn record_job(
        &self,
        profile: &str,
        channel: &str,
        params: &Value,
        kind: &str,
    ) -> Result<String> {
        let process = params["processId"]
            .as_str()
            .ok_or_else(|| Fault::new("INVALID_EXECUTION", "process ID missing"))?;
        let thread = params
            .pointer("/env/CODEX_THREAD_ID")
            .and_then(Value::as_str)
            .or_else(|| {
                params
                    .pointer("/auditMetadata/conversationId")
                    .and_then(Value::as_str)
            });
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO jobs(id,process_id,kind,profile,channel,thread,cwd,state) VALUES(?,?,?,?,?,?,?,'starting')")
            .bind(&id).bind(process).bind(kind).bind(profile).bind(channel).bind(thread).bind(params["cwd"].as_str().unwrap_or("")).execute(&self.pool).await.checked("JOB_CONFLICT","cannot persist process identity")?;
        Ok(id)
    }

    pub async fn process_job(&self, channel: &str, process: &str) -> Result<Option<String>> {
        sqlx::query_scalar(
            "SELECT id FROM jobs WHERE channel=? AND process_id=? ORDER BY rowid DESC LIMIT 1",
        )
        .bind(channel)
        .bind(process)
        .fetch_optional(&self.pool)
        .await
        .checked("STORAGE_ERROR", "cannot resolve execution process")
    }

    pub async fn set_job_state(&self, id: &str, state: &str, exit: Option<i64>) -> Result<()> {
        // Exit notifications may precede process/start's response. Terminal
        // states must not be resurrected by that late acknowledgement.
        sqlx::query("UPDATE jobs SET state=?,exit_code=?,updated_at=unixepoch() WHERE id=? AND state IN ('starting','running') AND (?<>'running' OR state='starting')")
            .bind(state)
            .bind(exit)
            .bind(id)
            .bind(state)
            .execute(&self.pool)
            .await
            .checked("STORAGE_ERROR", "cannot update execution process")?;
        Ok(())
    }
    pub async fn record_output(&self, channel: &str, params: &Value) -> Result<()> {
        let Some(process) = params["processId"].as_str() else {
            return Ok(());
        };
        let Some(id) = self.process_job(channel, process).await? else {
            return Ok(());
        };
        let encoded =
            serde_json::to_string(params).checked("PROTOCOL_ERROR", "invalid process output")?;
        let mut tx = self
            .pool
            .begin()
            .await
            .checked("STORAGE_ERROR", "cannot save process output")?;
        let changed=sqlx::query("UPDATE jobs SET output_bytes=output_bytes+?,updated_at=unixepoch() WHERE id=? AND output_bytes+?<=67108864")
            .bind(encoded.len() as i64).bind(&id).bind(encoded.len() as i64).execute(&mut *tx).await.checked("STORAGE_ERROR","cannot account process output")?;
        if changed.rows_affected() > 0 {
            sqlx::query("INSERT INTO job_output(job,record) VALUES(?,?)")
                .bind(&id)
                .bind(encoded)
                .execute(&mut *tx)
                .await
                .checked("STORAGE_ERROR", "cannot save process output")?;
        } else {
            sqlx::query("UPDATE jobs SET output_truncated=1 WHERE id=?")
                .bind(&id)
                .execute(&mut *tx)
                .await
                .checked("STORAGE_ERROR", "cannot mark truncated output")?;
        }
        tx.commit()
            .await
            .checked("STORAGE_ERROR", "cannot commit process output")
    }
    pub async fn jobs(&self, profile: &str, thread: Option<&str>) -> Result<Vec<Job>> {
        let rows:Vec<JobRow>=sqlx::query_as("SELECT id,process_id,kind,channel,thread,cwd,state,exit_code,created_at,updated_at,output_truncated FROM jobs WHERE profile=? AND kind='command' AND (? IS NULL OR thread=?) ORDER BY created_at DESC,id LIMIT 1000")
            .bind(profile).bind(thread).bind(thread).fetch_all(&self.pool).await.checked("STORAGE_ERROR","cannot list processes")?;
        Ok(rows.into_iter().map(decode).collect())
    }

    pub async fn job(&self, profile: &str, id: &str) -> Result<Job> {
        let row:Option<JobRow>=sqlx::query_as("SELECT id,process_id,kind,channel,thread,cwd,state,exit_code,created_at,updated_at,output_truncated FROM jobs WHERE profile=? AND id=?")
            .bind(profile).bind(id).fetch_optional(&self.pool).await.checked("STORAGE_ERROR","cannot read process")?;
        row.map(decode).ok_or_else(|| {
            Fault::new(
                "JOB_NOT_FOUND",
                "process does not belong to this execution environment",
            )
        })
    }
    pub async fn job_output(&self, profile: &str, id: &str, after: i64) -> Result<Value> {
        let job = self.job(profile, id).await?;
        let rows:Vec<(i64,String)>=sqlx::query_as("SELECT cursor,record FROM job_output WHERE job=? AND cursor>? ORDER BY cursor LIMIT 64")
            .bind(id).bind(after).fetch_all(&self.pool).await.checked("STORAGE_ERROR","cannot read process output")?;
        let mut next = after;
        let mut bytes = 0;
        let mut chunks = Vec::<Value>::new();
        for (cursor, record) in rows {
            if !chunks.is_empty() && bytes + record.len() > remote_codex_protocol::MAX_FRAME / 2 {
                break;
            }
            bytes += record.len();
            next = cursor;
            chunks.push(
                serde_json::from_str(&record)
                    .checked("STORAGE_ERROR", "invalid stored process output")?,
            );
        }
        Ok(json!({"job":job,"chunks":chunks,"next_cursor":next}))
    }
}
