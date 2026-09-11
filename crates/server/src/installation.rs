//! Replace an idle supervisor without changing its persisted installation identity.
use crate::{Checked, Result, paths};
use remote_codex_protocol::{Call, Fault, Frame, Hello, Request, ServiceActivity, VERSION, codec};
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};
use tokio::{io::BufReader, net::UnixStream};

pub(crate) fn build_id() -> Result<String> {
    let path =
        std::env::current_exe().checked("SERVICE_BUILD", "cannot locate service executable")?;
    let bytes = std::fs::read(path).checked("SERVICE_BUILD", "cannot read service executable")?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub enum Preparation {
    Ready,
    Start,
    Waiting(ServiceActivity),
}

pub async fn prepare(root: &Path, socket: &Path) -> Result<Preparation> {
    let Ok(mut stream) = UnixStream::connect(socket).await else {
        return Ok(Preparation::Start);
    };
    let credentials = stream
        .peer_cred()
        .checked("SERVICE_UPDATE", "cannot verify running service")?;
    if credentials.uid() != nix::unistd::geteuid().as_raw() {
        return Err(Fault::new(
            "SERVICE_UPDATE",
            "running service has another owner",
        ));
    }
    let call = Call {
        protocol: VERSION,
        id: "service-build".into(),
        profile: "maintenance".into(),
        expected_identity: None,
        request: Request::Hello,
    };
    codec::write(&mut stream, &call)
        .await
        .checked("SERVICE_UPDATE", "cannot inspect running service")?;
    let frame = tokio::time::timeout(
        Duration::from_secs(3),
        codec::Reader::new(BufReader::new(stream)).next::<Frame>(),
    )
    .await
    .checked("SERVICE_UPDATE", "running service did not respond")?
    .checked("SERVICE_UPDATE", "cannot read running service identity")?;
    let Some(Frame::Result(value)) = frame else {
        return Err(Fault::new(
            "SERVICE_UPDATE",
            "running service handshake failed",
        ));
    };
    let hello: Hello = serde_json::from_value(value)
        .checked("SERVICE_UPDATE", "invalid running service identity")?;
    if hello.build_id == build_id()? {
        return Ok(Preparation::Ready);
    }
    if hello
        .capabilities
        .iter()
        .any(|c| c == remote_codex_protocol::IDLE_RETIREMENT_CAPABILITY)
    {
        retire_idle(socket, &hello.identity).await?;
    }
    let pid = credentials.pid().ok_or_else(|| {
        Fault::new(
            "SERVICE_UPDATE",
            "automatic service replacement requires Linux peer process credentials",
        )
    })?;
    replace_idle(root, &hello.identity, pid).await
}

async fn retire_idle(socket: &Path, identity: &str) -> Result<()> {
    let mut stream = UnixStream::connect(socket)
        .await
        .checked("SERVICE_UPDATE", "cannot request idle cleanup")?;
    codec::write(
        &mut stream,
        &Call {
            protocol: VERSION,
            id: "service-retire-idle".into(),
            profile: "maintenance".into(),
            expected_identity: Some(identity.into()),
            request: Request::PrepareUpdate,
        },
    )
    .await
    .checked("SERVICE_UPDATE", "cannot request idle cleanup")?;
    let frame = tokio::time::timeout(
        Duration::from_secs(3),
        codec::Reader::new(BufReader::new(stream)).next::<Frame>(),
    )
    .await
    .checked("SERVICE_UPDATE", "idle cleanup request timed out")?
    .checked("SERVICE_UPDATE", "invalid idle cleanup response")?;
    match frame {
        Some(Frame::Result(_)) => Ok(()),
        Some(Frame::Error(error)) => Err(error),
        _ => Err(Fault::new(
            "SERVICE_UPDATE",
            "idle cleanup was not acknowledged",
        )),
    }
}

async fn replace_idle(root: &Path, identity: &str, pid: i32) -> Result<Preparation> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    let options = SqliteConnectOptions::new()
        .filename(root.join("execution.sqlite3"))
        .busy_timeout(Duration::from_secs(3));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .checked("SERVICE_UPDATE", "cannot inspect service activity")?;
    // No accepted operation can enter the dispatcher while this write lock is held.
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await.checked(
        "SERVICE_UPDATE_BUSY",
        "service is busy; retry after active sessions finish",
    )?;
    let stored: String = sqlx::query_scalar("SELECT id FROM identity WHERE singleton=1")
        .fetch_one(&mut *tx)
        .await
        .checked(
            "SERVICE_UPDATE",
            "cannot inspect stored installation identity",
        )?;
    if stored != identity {
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
        return Ok(Preparation::Waiting(ServiceActivity {
            channels,
            jobs,
            transfers: i64::from(transfer_lock.is_err()),
        }));
    }
    let _transfer_lock = transfer_lock;
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
            return Ok(Preparation::Start);
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
