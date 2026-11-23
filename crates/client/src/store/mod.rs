mod access;
mod codec;
mod configuration;
mod connections;
mod credentials;
mod initialization;
mod message_times;
mod schema;
mod servers;
mod sessions;
mod ssh_credentials;
mod transfers;
mod trust;
mod workspaces;

use serde::Serialize;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::config::{ConfigLayer, EffectiveConfig};
use crate::{ClientError, Result};

pub(crate) use connections::ConnectionRecord;
pub(crate) use servers::ServerAccess;
pub(crate) use sessions::CachedSession;

#[derive(Clone)]
pub(crate) struct LocalStore {
    pool: SqlitePool,
    pub(crate) directory: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Revision {
    pub(crate) saved: i64,
}

pub(crate) struct ConfigSnapshot {
    pub(crate) revision: Revision,
    pub(crate) layer: ConfigLayer,
    pub(crate) effective: EffectiveConfig,
}

pub(crate) use remote_codex_core::config::{ConfigItem, ConfigReport};

impl LocalStore {
    pub(crate) async fn open(directory: &Path) -> Result<Self> {
        private_directory(directory)?;
        let _initialization = initialization::lock(directory).await?;
        let path = directory.join("state.sqlite3");
        match remote_codex_platform::fs::open_private(
            &path,
            remote_codex_platform::fs::Creation::New,
        ) {
            Ok(file) => file.sync_all()?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let meta = fs::symlink_metadata(&path)?;
        if !meta.is_file() || remote_codex_platform::fs::check_private(&path).is_err() {
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
        if let Err(error) = schema::initialize(&pool).await {
            pool.close().await;
            return Err(error);
        }
        Ok(Self {
            pool,
            directory: directory.to_owned(),
        })
    }

    pub(crate) async fn close(&self) {
        self.pool.close().await;
    }

    pub(crate) async fn begin_write(&self) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        schema::ensure_current(&mut tx).await?;
        Ok(tx)
    }
}

pub(crate) fn default_data_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("REMOTE_CODEX_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(windows)]
    let directory =
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or(ClientError::PrivatePath)?)
            .join("remote-codex");
    #[cfg(unix)]
    let home = std::env::var_os("HOME").ok_or(ClientError::PrivatePath)?;
    #[cfg(target_os = "macos")]
    let directory = PathBuf::from(home).join("Library/Application Support/remote-codex");
    #[cfg(all(unix, not(target_os = "macos")))]
    let directory = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".local/share"))
        .join("remote-codex");
    Ok(directory)
}

pub(crate) fn private_directory(path: &Path) -> Result<()> {
    remote_codex_platform::fs::private_dir(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            ClientError::PrivatePath
        } else {
            error.into()
        }
    })
}
