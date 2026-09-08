use sqlx::{Connection, SqliteConnection, SqlitePool, sqlite::SqliteConnectOptions};

use crate::{ClientError, Result};

const APPLICATION_ID: i64 = 0x52434458;
const SCHEMA_VERSION: i64 = 7;

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
        if app == APPLICATION_ID && (1..=SCHEMA_VERSION).contains(&version) {
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

pub(super) async fn initialize(pool: &SqlitePool, directory: &std::path::Path) -> Result<()> {
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
    if (1..SCHEMA_VERSION).contains(&version) && app == APPLICATION_ID {
        super::backup::create(pool, directory, version).await?;
        if version == 1 {
            sqlx::raw_sql(include_str!("migration_v2.sql"))
                .execute(&mut *tx)
                .await?;
        }
        if version <= 2 {
            sqlx::raw_sql(include_str!("migration_v3.sql"))
                .execute(&mut *tx)
                .await?;
        }
        if version <= 3 {
            sqlx::raw_sql(include_str!("migration_v4.sql"))
                .execute(&mut *tx)
                .await?;
        }
        if version < 6 {
            sqlx::raw_sql(include_str!("migration_v6.sql"))
                .execute(&mut *tx)
                .await?;
        }
        sqlx::raw_sql(include_str!("migration_v7.sql"))
            .execute(&mut *tx)
            .await?;
        validate_migration(&mut tx).await?;
        sqlx::query(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))
            .execute(&mut *tx)
            .await?;
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
    sqlx::raw_sql(include_str!("schema.sql"))
        .execute(&mut *tx)
        .await?;
    sqlx::raw_sql(include_str!("migration_v6.sql"))
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO installation(id) VALUES (?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query(&format!("PRAGMA application_id = {APPLICATION_ID}"))
        .execute(&mut *tx)
        .await?;
    sqlx::query(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn validate_migration(connection: &mut SqliteConnection) -> Result<()> {
    if sqlx::query("PRAGMA foreign_key_check")
        .fetch_optional(&mut *connection)
        .await?
        .is_some()
    {
        return Err(ClientError::Schema);
    }
    let servers: Vec<String> = sqlx::query_scalar("SELECT id FROM connections")
        .fetch_all(&mut *connection)
        .await?;
    for server in servers {
        let layer = super::codec::load(connection, &server).await?;
        crate::config::resolve(&layer)?;
    }
    let missing: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM settings s WHERE representation='secret' AND NOT EXISTS(SELECT 1 FROM credentials c WHERE c.id=s.value AND c.state='active')) OR EXISTS(SELECT 1 FROM retained_legacy_credentials r WHERE NOT EXISTS(SELECT 1 FROM credentials c WHERE c.id=r.id AND c.state='active'))",
    ).fetch_one(connection).await?;
    if missing {
        return Err(ClientError::Credentials);
    }
    Ok(())
}
