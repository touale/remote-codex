use sqlx::{Connection, SqliteConnection, SqlitePool, sqlite::SqliteConnectOptions};

use crate::{ClientError, Result};

const APPLICATION_ID: i64 = 0x52434458;
const SCHEMA_VERSION: i64 = 13;

pub(super) async fn preflight(path: &std::path::Path) -> Result<()> {
    let mut connection =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(path).read_only(true))
            .await?;
    let result = async {
        let app: i64 = sqlx::query_scalar("PRAGMA application_id")
            .fetch_one(&mut connection)
            .await?;
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut connection)
            .await?;
        if app == APPLICATION_ID && version == SCHEMA_VERSION {
            return Ok(());
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(&mut connection)
        .await?;
        if app == 0 && version == 0 && count == 0 {
            Ok(())
        } else {
            Err(ClientError::Schema)
        }
    }
    .await;
    connection.close().await?;
    result
}

pub(super) async fn ensure_current(connection: &mut SqliteConnection) -> Result<()> {
    let app: i64 = sqlx::query_scalar("PRAGMA application_id")
        .fetch_one(&mut *connection)
        .await?;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(connection)
        .await?;
    if app == APPLICATION_ID && version == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(ClientError::Schema)
    }
}

pub(super) async fn initialize(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let app: i64 = sqlx::query_scalar("PRAGMA application_id")
        .fetch_one(&mut *tx)
        .await?;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *tx)
        .await?;
    if version == SCHEMA_VERSION && app == APPLICATION_ID {
        tx.commit().await?;
        return Ok(());
    }
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_one(&mut *tx)
    .await?;
    if version != 0 || app != 0 || tables != 0 {
        return Err(ClientError::Schema);
    }
    sqlx::Executor::execute(&mut *tx, include_str!("schema.sql")).await?;
    sqlx::query("INSERT INTO installation(id) VALUES (?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::QueryBuilder::<sqlx::Sqlite>::new("PRAGMA application_id = ")
        .push(APPLICATION_ID)
        .build()
        .execute(&mut *tx)
        .await?;
    sqlx::QueryBuilder::<sqlx::Sqlite>::new("PRAGMA user_version = ")
        .push(SCHEMA_VERSION)
        .build()
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
