use sqlx::{Row, sqlite::SqliteRow};

use super::LocalStore;
use crate::{
    ClientError, Result,
    connection::{SshEndpoint, validate_name},
};

#[derive(Clone, Debug)]
pub(crate) struct ConnectionRecord {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) endpoint: SshEndpoint,
    pub(crate) runtime: Option<crate::runtime::RuntimeInfo>,
}

impl LocalStore {
    pub(crate) async fn find_connection(&self, name: &str) -> Result<ConnectionRecord> {
        let row = sqlx::query("SELECT * FROM connections WHERE connections.name=?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(ClientError::NotFound)?;
        decode(row)
    }

    pub(crate) async fn connection_by_id(&self, id: &str) -> Result<ConnectionRecord> {
        let row = sqlx::query("SELECT * FROM connections WHERE connections.id=?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(ClientError::NotFound)?;
        decode(row)
    }

    pub(crate) async fn list_connections(&self) -> Result<Vec<ConnectionRecord>> {
        sqlx::query("SELECT * FROM connections ORDER BY connections.name")
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
        if let Some(row) = sqlx::query("SELECT * FROM connections WHERE connections.name=?")
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
            && let Some(row) = sqlx::query(
                "SELECT * FROM connections WHERE endpoint=? ORDER BY connections.name LIMIT 1",
            )
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

    /// Remember a verified package for offline lookup; execution is selected per channel.
    pub(crate) async fn remember_runtime(
        &self,
        id: &str,
        runtime: &crate::runtime::RuntimeInfo,
    ) -> Result<()> {
        let mut tx = self.begin_write().await?;
        let rows = sqlx::query("UPDATE connections SET runtime=? WHERE id=?")
            .bind(serde_json::to_string(runtime)?)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if rows != 1 {
            return Err(ClientError::NotFound);
        }
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
        runtime: runtime
            .map(|text| serde_json::from_str(&text))
            .transpose()?,
    })
}
