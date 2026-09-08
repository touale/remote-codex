use serde::Serialize;
use sqlx::{Row, sqlite::SqliteRow};

use super::LocalStore;
use crate::{
    ClientError, Result,
    connection::{SshEndpoint, validate_name},
};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ConnectionRecord {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) endpoint: SshEndpoint,
    pub(crate) saved_revision: i64,
    pub(crate) runtime: Option<crate::runtime::RuntimeInfo>,
}

const SELECT: &str = "SELECT connections.*,server_revisions.revision FROM connections JOIN server_revisions ON connections.id=server_revisions.id";

impl LocalStore {
    pub(crate) async fn find_connection(&self, name: &str) -> Result<ConnectionRecord> {
        let row = sqlx::query(&format!("{SELECT} WHERE connections.name=?"))
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(ClientError::NotFound)?;
        decode(row)
    }

    pub(crate) async fn connection_by_id(&self, id: &str) -> Result<ConnectionRecord> {
        let row = sqlx::query(&format!("{SELECT} WHERE connections.id=?"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(ClientError::NotFound)?;
        decode(row)
    }

    pub(crate) async fn list_connections(&self) -> Result<Vec<ConnectionRecord>> {
        sqlx::query(&format!("{SELECT} ORDER BY connections.name"))
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(decode)
            .collect()
    }

    pub(crate) async fn save_connection(
        &self,
        endpoint: &SshEndpoint,
        name: Option<&str>,
    ) -> Result<ConnectionRecord> {
        endpoint.validate()?;
        let explicit = name.is_some();
        let name = name
            .map(str::to_owned)
            .unwrap_or_else(|| endpoint.default_name());
        validate_name(&name)?;
        let encoded = serde_json::to_string(endpoint)?;
        let mut tx = self.begin_write().await?;
        if let Some(row) = sqlx::query(&format!("{SELECT} WHERE connections.name=?"))
            .bind(&name)
            .fetch_optional(&mut *tx)
            .await?
        {
            let record = decode(row)?;
            if record.endpoint != *endpoint {
                return Err(ClientError::NameConflict);
            }
            tx.commit().await?;
            return Ok(record);
        }
        if !explicit
            && let Some(row) = sqlx::query(&format!(
                "{SELECT} WHERE endpoint=? ORDER BY connections.name LIMIT 1"
            ))
            .bind(&encoded)
            .fetch_optional(&mut *tx)
            .await?
        {
            let record = decode(row)?;
            tx.commit().await?;
            return Ok(record);
        }
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO server_revisions(id) VALUES (?)")
            .bind(&id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO connections(id,name,endpoint) VALUES (?,?,?)")
            .bind(&id)
            .bind(&name)
            .bind(encoded)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.connection_by_id(&id).await
    }

    pub(crate) async fn record_runtime(
        &self,
        id: &str,
        expected: i64,
        runtime: &crate::runtime::RuntimeInfo,
    ) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let changed = sqlx::query("UPDATE connections SET runtime=? WHERE id=? AND EXISTS (SELECT 1 FROM server_revisions WHERE id=? AND revision=?)")
            .bind(serde_json::to_string(runtime)?).bind(id).bind(id).bind(expected).execute(&mut *tx).await?.rows_affected();
        if changed != 1 {
            return Err(ClientError::RevisionConflict);
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn activate_runtime(
        &self,
        id: &str,
        expected: i64,
        runtime: &crate::runtime::RuntimeInfo,
    ) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let current: Option<String> =
            sqlx::query_scalar("SELECT runtime FROM connections WHERE id=?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
        let previous: Option<crate::runtime::RuntimeInfo> =
            current.map(|s| serde_json::from_str(&s)).transpose()?;
        let changed = previous.as_ref().is_none_or(|old| {
            old.version != runtime.version
                || old.executable != runtime.executable
                || old.archive_sha256 != runtime.archive_sha256
        });
        let rows=sqlx::query("UPDATE server_revisions SET revision=revision+? WHERE id=? AND revision=? AND revision<9223372036854775807")
            .bind(i64::from(changed)).bind(id).bind(expected).execute(&mut *tx).await?.rows_affected();
        if rows != 1 {
            return Err(ClientError::RevisionConflict);
        }
        sqlx::query("UPDATE connections SET runtime=? WHERE id=?")
            .bind(serde_json::to_string(runtime)?)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn decode(row: SqliteRow) -> Result<ConnectionRecord> {
    let endpoint: SshEndpoint = serde_json::from_str(row.try_get("endpoint")?)?;
    endpoint.validate()?;
    let runtime: Option<String> = row.try_get("runtime")?;
    Ok(ConnectionRecord {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        endpoint,
        saved_revision: row.try_get("revision")?,
        runtime: runtime
            .map(|text| serde_json::from_str(&text))
            .transpose()?,
    })
}
