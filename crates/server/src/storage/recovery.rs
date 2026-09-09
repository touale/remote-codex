use super::Store;
use crate::{Checked, Result};
use remote_codex_protocol::{ExecutionStatus, Fault};

pub(super) fn boot_id() -> Result<String> {
    #[cfg(target_os = "linux")]
    {
        let id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .checked("BOOT_ID", "cannot identify host boot")?;
        uuid::Uuid::parse_str(id.trim())
            .map_err(|_| Fault::new("BOOT_ID", "invalid host boot identity"))?;
        Ok(id.trim().into())
    }
    #[cfg(not(target_os = "linux"))]
    Ok(String::new())
}

impl Store {
    pub async fn inspect_execution(&self, profile: &str, channel: &str) -> Result<ExecutionStatus> {
        let row: Option<(String, String, String, Option<String>, i64)> = sqlx::query_as(
            "SELECT service_instance,boot_id,state,lost_reason,replay_floor FROM execution_channels WHERE id=? AND profile=?")
            .bind(channel).bind(profile).fetch_optional(&self.pool).await
            .checked("STORAGE_ERROR", "cannot inspect execution recovery")?;
        let (service_instance, boot_id, state, reason, replay_floor) = row.ok_or_else(|| {
            Fault::new(
                "EXECUTION_NOT_FOUND",
                "execution does not belong to this profile",
            )
        })?;
        let unknown_operations = sqlx::query_scalar(
            "SELECT count(*) FROM operations WHERE substr(id,1,instr(id,'/')-1)=? AND result IS NULL",
        )
        .bind(channel)
        .fetch_one(&self.pool)
        .await
        .checked("STORAGE_ERROR", "cannot inspect operation outcomes")?;
        let evidence_available = reason.as_deref() != Some("evidence_expired");
        Ok(ExecutionStatus {
            channel: channel.into(),
            service_instance,
            boot_id,
            state,
            reason,
            replay_floor,
            unknown_operations,
            evidence_available,
        })
    }
}
