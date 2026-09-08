use super::private_directory;
use crate::{ClientError, Result};
use sqlx::SqlitePool;
use std::{fs, path::Path};

/// The caller holds BEGIN IMMEDIATE and has not changed the source database.
/// A second connection reads its committed WAL snapshot while other writers wait.
pub(super) async fn create(pool: &SqlitePool, directory: &Path, version: i64) -> Result<()> {
    let root = directory.join("backups");
    private_directory(&root)?;
    let temporary = tempfile::NamedTempFile::new_in(&root)?;
    let destination = temporary.path().to_str().ok_or(ClientError::PrivatePath)?;
    sqlx::query("VACUUM main INTO ?")
        .bind(destination)
        .execute(pool)
        .await?;
    temporary.as_file().sync_all()?;
    let target = root.join(format!("state-v{version}-{}.sqlite3", uuid::Uuid::new_v4()));
    temporary
        .persist_noclobber(target)
        .map_err(|error| error.error)?;
    fs::File::open(root)?.sync_all()?;
    Ok(())
}
