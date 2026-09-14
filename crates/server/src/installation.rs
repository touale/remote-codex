//! Replace an idle supervisor without changing its persisted installation identity.
use crate::{Checked, Result, paths};
use remote_codex_protocol::{Fault, Request, ServiceActivity};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};

mod handshake;
mod process;
pub use handshake::ready;

pub fn build_id() -> Result<String> {
    let path =
        std::env::current_exe().checked("SERVICE_BUILD", "cannot locate service executable")?;
    let bytes = std::fs::read(path).checked("SERVICE_BUILD", "cannot read service executable")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub enum Preparation {
    Ready,
    Start(Option<String>),
    Waiting(ServiceActivity),
}

pub async fn prepare(root: &Path, socket: &Path, build: &str) -> Result<Preparation> {
    let Some(mut stream) = handshake::connect(socket).await? else {
        return Ok(Preparation::Start(None));
    };
    let credentials = stream
        .peer_cred()
        .checked("SERVICE_UPDATE", "cannot verify running service")?;
    let hello = match handshake::hello(&mut stream).await {
        Ok(hello) if hello.build_id == build => return Ok(Preparation::Ready),
        Ok(hello) => Some(hello),
        Err(error) if error.code == "PROTOCOL_MISMATCH" => None,
        Err(error) => return Err(error),
    };
    let pid = credentials.pid().ok_or_else(|| {
        Fault::new(
            "SERVICE_UPDATE",
            "automatic service replacement requires Linux peer process credentials",
        )
    })?;
    if let Some(hello) = &hello {
        if hello
            .capabilities
            .iter()
            .any(|c| c == remote_codex_protocol::IDLE_RETIREMENT_CAPABILITY)
        {
            let mut stream = handshake::connect(socket).await?.ok_or_else(|| {
                Fault::new(
                    "SERVICE_UPDATE",
                    "running service stopped before idle cleanup",
                )
            })?;
            handshake::call(&mut stream, Request::PrepareUpdate, Some(&hello.identity)).await?;
        }
    } else {
        process::verify(root, pid)?;
    }
    replace_idle(
        root,
        hello.as_ref().map(|value| value.identity.as_str()),
        pid,
    )
    .await
}

async fn replace_idle(root: &Path, identity: Option<&str>, pid: i32) -> Result<Preparation> {
    use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
    let database = root.join("execution.sqlite3");
    std::fs::symlink_metadata(&database)
        .checked("SERVICE_UPDATE", "cannot inspect installation database")?;
    let _database_file = paths::file(&database)?;
    let options = SqliteConnectOptions::new()
        .filename(database)
        .busy_timeout(Duration::from_secs(3));
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .checked("SERVICE_UPDATE", "cannot inspect service activity")?;
    // No accepted operation can enter the dispatcher while this write lock is held.
    let mut tx = connection.begin_with("BEGIN IMMEDIATE").await.checked(
        "SERVICE_UPDATE_BUSY",
        "service is busy; retry after active sessions finish",
    )?;
    let application: i64 = sqlx::query_scalar("PRAGMA application_id")
        .fetch_one(&mut *tx)
        .await
        .checked("SERVICE_UPDATE", "cannot inspect database identity")?;
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut *tx)
        .await
        .checked("SERVICE_UPDATE", "cannot inspect database version")?;
    // The known pre-runtime-selection installation has schema 2. A protocol
    // rejection from any other schema must not trigger an assumed downgrade.
    if application != 0x52435356
        || !matches!(version, 2 | 3)
        || (identity.is_none() && version != 2)
    {
        return Err(Fault::new(
            "UNSUPPORTED_SCHEMA",
            "running service is not a supported upgrade source",
        ));
    }
    let stored: String = sqlx::query_scalar("SELECT id FROM identity WHERE singleton=1")
        .fetch_one(&mut *tx)
        .await
        .checked(
            "SERVICE_UPDATE",
            "cannot inspect stored installation identity",
        )?;
    if identity.is_some_and(|expected| stored != expected) {
        return Err(Fault::new(
            "SERVICE_UPDATE",
            "installation identity changed during update",
        ));
    }
    let (channels, jobs): (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM execution_channels WHERE state='running'), (SELECT count(*) FROM jobs WHERE state IN ('starting','running'))")
        .fetch_one(&mut *tx).await.checked("SERVICE_UPDATE", "cannot inspect active execution")?;
    let transfer_file = paths::file(&root.join("transfer.lock"))?;
    let transfer_lock =
        nix::fcntl::Flock::lock(transfer_file, nix::fcntl::FlockArg::LockExclusiveNonblock);
    if channels != 0 || jobs != 0 || transfer_lock.is_err() {
        let activity = ServiceActivity {
            channels,
            jobs,
            transfers: i64::from(transfer_lock.is_err()),
        };
        return if identity.is_none() {
            Err(activity.fault())
        } else {
            Ok(Preparation::Waiting(activity))
        };
    }
    let _transfer_lock = transfer_lock;
    if identity.is_none() {
        process::verify(root, pid)?;
    }
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGTERM,
    )
    .checked("SERVICE_UPDATE", "cannot stop idle service")?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(6);
    loop {
        let file = paths::file(&root.join("service.lock"))?;
        if let Ok(lock) = nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock)
        {
            tx.commit()
                .await
                .checked("SERVICE_UPDATE", "cannot finish idle service replacement")?;
            drop(lock);
            return Ok(Preparation::Start(Some(stored)));
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Fault::new(
                "SERVICE_UPDATE",
                "idle service did not stop; retry before opening a session",
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
