use crate::{Checked, Result};
use remote_codex_protocol::Fault;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use std::path::Path;

pub(super) async fn check(path: &Path) -> Result<()> {
    let mut connection =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(path).read_only(true))
            .await
            .checked("STORAGE_ERROR", "cannot inspect database")?;
    let result = async {
        let app: i64 = sqlx::query_scalar("PRAGMA application_id")
            .fetch_one(&mut connection)
            .await
            .checked("STORAGE_ERROR", "cannot read database identity")?;
        let version: i64 = sqlx::query_scalar("PRAGMA user_version")
            .fetch_one(&mut connection)
            .await
            .checked("STORAGE_ERROR", "cannot read database version")?;
        let tables: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(&mut connection)
        .await
        .checked("STORAGE_ERROR", "cannot inspect schema")?;
        if (app == 0x52435356 && matches!(version, 2 | 3))
            || (app == 0 && version == 0 && tables == 0)
        {
            Ok(())
        } else {
            Err(Fault::new(
                "UNSUPPORTED_SCHEMA",
                "database requires another service version or belongs to another application",
            ))
        }
    }
    .await;
    connection
        .close()
        .await
        .checked("STORAGE_ERROR", "cannot close database inspection")?;
    result
}
