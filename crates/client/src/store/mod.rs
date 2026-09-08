mod backup;
mod codec;
mod configuration;
mod connections;
mod credentials;
mod initialization;
mod schema;
mod servers;
mod sessions;

use serde::Serialize;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use crate::config::{ConfigLayer, EffectiveConfig, SettingView};
use crate::{ClientError, Result};

pub use connections::ConnectionRecord;
pub use servers::ServerAccess;
pub use sessions::CachedSession;

#[derive(Clone)]
pub struct LocalStore {
    pub(crate) pool: SqlitePool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Revision {
    pub saved: i64,
}

pub struct ConfigSnapshot {
    pub revision: Revision,
    pub layer: ConfigLayer,
    pub effective: EffectiveConfig,
}

#[derive(Debug, Serialize)]
pub struct ConfigReport {
    pub schema_version: u32,
    pub server_id: String,
    pub saved_revision: i64,
    pub applied_revision: Option<i64>,
    pub items: Vec<ConfigItem>,
}

#[derive(Debug, Serialize)]
pub struct ConfigItem {
    #[serde(flatten)]
    pub setting: SettingView,
    pub application_state: &'static str,
}

impl LocalStore {
    pub async fn open(directory: &Path) -> Result<Self> {
        private_directory(directory)?;
        let _initialization = initialization::lock(directory).await?;
        let path = directory.join("state.sqlite3");
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => file.sync_all()?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
            return Err(ClientError::PrivatePath);
        }
        if meta.len() > 0 {
            schema::preflight(&path).await?;
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(3));
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await?;
        if let Err(error) = schema::initialize(&pool, directory).await {
            pool.close().await;
            return Err(error);
        }
        Ok(Self { pool })
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    pub(crate) async fn begin_write(&self) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        schema::ensure_current(&mut tx).await?;
        Ok(tx)
    }
}

pub fn default_data_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("REMOTE_CODEX_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME").ok_or(ClientError::PrivatePath)?;
    #[cfg(target_os = "macos")]
    let directory = PathBuf::from(home).join("Library/Application Support/remote-codex");
    #[cfg(not(target_os = "macos"))]
    let directory = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".local/share"))
        .join("remote-codex");
    Ok(directory)
}

pub fn private_directory(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err(ClientError::PrivatePath);
    }
    Ok(())
}
