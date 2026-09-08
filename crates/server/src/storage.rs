mod execution;
mod jobs;
mod preflight;
mod retention;

use crate::{Checked, Result, paths};
use remote_codex_protocol::{Fault, RemoteConfig};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{path::Path, time::Duration};

#[derive(Clone)]
pub struct Store {
    pub(crate) pool: SqlitePool,
    pub identity: String,
}

impl Store {
    pub async fn open(root: &Path) -> Result<Self> {
        paths::private(root)?;
        let path = root.join("execution.sqlite3");
        paths::file(&path)?;
        preflight::check(&path).await?;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(3));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .checked("STORAGE_ERROR", "cannot open remote database")?;
        let mut tx = pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .checked("STORAGE_ERROR", "cannot initialize remote database")?;
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot read schema version")?;
        let application: i64 = sqlx::query_scalar("PRAGMA application_id")
            .fetch_one(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot read database identity")?;
        let tables: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(&mut *tx)
        .await
        .checked("STORAGE_ERROR", "cannot inspect database")?;
        if (application != 0x52435356 && (application != 0 || tables != 0))
            || (version != 0 && application == 0)
        {
            return Err(Fault::new(
                "UNSUPPORTED_SCHEMA",
                "database belongs to another application",
            ));
        }
        if version > 1 {
            return Err(Fault::new(
                "UNSUPPORTED_SCHEMA",
                "remote service database requires a newer version",
            ));
        }
        sqlx::raw_sql(include_str!("storage/schema.sql"))
            .execute(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot initialize remote schema")?;
        sqlx::query("INSERT OR IGNORE INTO identity(singleton,id) VALUES(1,?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .execute(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot save remote identity")?;
        sqlx::raw_sql("PRAGMA application_id=1380143958; PRAGMA user_version=1;")
            .execute(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot save schema version")?;
        let identity = sqlx::query_scalar("SELECT id FROM identity WHERE singleton=1")
            .fetch_one(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot read remote identity")?;
        sqlx::query("UPDATE execution_channels SET state='lost' WHERE state='running'")
            .execute(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot reconcile executors")?;
        sqlx::query("UPDATE jobs SET state='unknown' WHERE state IN ('starting','running')")
            .execute(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot reconcile jobs")?;
        tx.commit()
            .await
            .checked("STORAGE_ERROR", "cannot commit remote initialization")?;
        Ok(Self { pool, identity })
    }

    pub async fn save_config(&self, profile: &str, config: &RemoteConfig) -> Result<()> {
        let encoded = serde_json::to_string(config)
            .checked("INVALID_CONFIG", "invalid remote configuration")?;
        let changed = sqlx::query("INSERT INTO profiles(id,config,revision) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET config=excluded.config,revision=excluded.revision WHERE excluded.revision>profiles.revision OR (excluded.revision=profiles.revision AND excluded.config=profiles.config)")
            .bind(profile).bind(encoded).bind(config.revision).execute(&self.pool).await.checked("STORAGE_ERROR", "cannot persist remote configuration")?;
        if changed.rows_affected() != 1 {
            return Err(Fault::new(
                "REVISION_CONFLICT",
                "configuration revision is stale or conflicts with saved values",
            ));
        }
        Ok(())
    }

    pub async fn config(&self, profile: &str) -> Result<RemoteConfig> {
        let encoded: Option<String> = sqlx::query_scalar("SELECT config FROM profiles WHERE id=?")
            .bind(profile)
            .fetch_optional(&self.pool)
            .await
            .checked("STORAGE_ERROR", "cannot read remote configuration")?;
        serde_json::from_str(&encoded.ok_or_else(|| {
            Fault::new(
                "CONFIG_REQUIRED",
                "server configuration must be synchronized",
            )
        })?)
        .checked("INVALID_CONFIG", "invalid saved remote configuration")
    }

    pub async fn begin_operation(
        &self,
        id: &str,
        digest: &str,
    ) -> Result<Option<serde_json::Value>> {
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .checked("STORAGE_ERROR", "cannot record operation")?;
        let existing: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT digest,result FROM operations WHERE id=?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .checked("STORAGE_ERROR", "cannot inspect operation")?;
        if let Some((old, result)) = existing {
            if old != digest {
                return Err(Fault::new(
                    "REQUEST_CONFLICT",
                    "request ID was already used with different arguments",
                ));
            }
            return match result {
                Some(value) => serde_json::from_str(&value)
                    .map(Some)
                    .checked("STORAGE_ERROR", "invalid stored operation result"),
                None => Err(Fault::unknown(
                    "previous operation was dispatched; inspect remote jobs before retrying",
                )),
            };
        }
        sqlx::query("INSERT INTO operations(id,digest) VALUES(?,?)")
            .bind(id)
            .bind(digest)
            .execute(&mut *tx)
            .await
            .checked("STORAGE_ERROR", "cannot persist operation intent")?;
        tx.commit()
            .await
            .checked("STORAGE_ERROR", "cannot persist operation intent")?;
        Ok(None)
    }

    pub async fn complete_operation(&self, id: &str, result: &serde_json::Value) -> Result<()> {
        sqlx::query("UPDATE operations SET result=? WHERE id=?")
            .bind(result.to_string())
            .bind(id)
            .execute(&self.pool)
            .await
            .checked("STORAGE_ERROR", "cannot persist operation result")?;
        Ok(())
    }
}
