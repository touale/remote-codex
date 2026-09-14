use remote_codex_protocol::{Fault, Frame, Hello, VERSION};
use remote_codex_server::{installation, paths};
#[cfg(target_os = "linux")]
use serde_json::json;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::process::{Child, Command};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn private_directory() -> std::io::Result<tempfile::TempDir> {
    use std::os::unix::fs::PermissionsExt;
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
}

struct Fixture(tempfile::TempDir);
impl Fixture {
    fn build() -> TestResult<Self> {
        let fixture = Self(private_directory()?);
        let result = std::process::Command::new("rustc")
            .arg("--edition=2024")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/supervisor.rs"
            ))
            .arg("-o")
            .arg(fixture.0.path().join("remote-codex-server"))
            .output()?;
        if !result.status.success() {
            return Err(String::from_utf8_lossy(&result.stderr).into_owned().into());
        }
        Ok(fixture)
    }

    async fn spawn(&self, state: &Path, socket: &Path, response: &str) -> TestResult<Child> {
        paths::private(state)?;
        let mut child = Command::new(self.0.path().join("remote-codex-server"))
            .arg("--state")
            .arg(state)
            .arg("serve")
            .env("RC_TEST_SOCKET", socket)
            .env("RC_TEST_RESPONSE", response)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        tokio::time::timeout(Duration::from_secs(5), async {
            while !socket.exists() {
                if child.try_wait()?.is_some() {
                    return Err("fixture exited".into());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await??;
        Ok(child)
    }
}

fn response(protocol: u32, identity: &str, build: &str) -> TestResult<String> {
    Ok(serde_json::to_string(&Frame::Result(
        serde_json::to_value(Hello {
            protocol,
            identity: identity.into(),
            home: "/tmp".into(),
            user: "fixture".into(),
            version: "fixture".into(),
            capabilities: vec![],
            build_id: build.into(),
            service_instance: "fixture-instance".into(),
            boot_id: "fixture-boot".into(),
        })?,
    ))?)
}

fn assert_current_protocol(root: &Path) -> TestResult {
    let requests = std::fs::read_to_string(root.join("requests.jsonl"))?;
    assert!(!requests.is_empty());
    for line in requests.lines() {
        let request: serde_json::Value = serde_json::from_str(line)?;
        assert_eq!(request["protocol"], VERSION);
    }
    Ok(())
}

#[tokio::test]
async fn handshake_errors_are_preserved_without_stopping_the_peer() -> TestResult {
    let fixture = Fixture::build()?;
    let fault = serde_json::to_string(&Frame::Error(Fault::new(
        "INVALID_PROFILE",
        "original reason",
    )))?;
    let cases = [
        (fault.as_str(), "INVALID_PROFILE", "original reason"),
        ("closed", "SERVICE_UPDATE", "closed"),
        (r#"{"type":"heartbeat"}"#, "SERVICE_UPDATE", "unexpected"),
        ("timeout", "SERVICE_UPDATE", "did not respond"),
    ];
    for (response, code, message) in cases {
        let root = private_directory()?;
        let socket = paths::socket(root.path())?;
        let mut child = fixture.spawn(root.path(), &socket, response).await?;
        let error = installation::prepare(root.path(), &socket, "expected-build")
            .await
            .err()
            .ok_or("expected handshake failure")?;
        assert_eq!(error.code, code);
        assert!(error.message.contains(message));
        assert!(child.try_wait()?.is_none());
        assert_current_protocol(root.path())?;
        child.kill().await?;
        std::fs::remove_file(socket)?;
    }
    Ok(())
}

#[tokio::test]
async fn readiness_requires_protocol_identity_and_build() -> TestResult {
    let fixture = Fixture::build()?;
    for (protocol, identity, build, succeeds) in [
        (VERSION, "expected-identity", "expected-build", true),
        (VERSION, "other-identity", "expected-build", false),
        (VERSION, "expected-identity", "other-build", false),
        (VERSION + 1, "expected-identity", "expected-build", false),
    ] {
        let root = private_directory()?;
        let socket = paths::socket(root.path())?;
        assert!(!installation::ready(&socket, Some("expected-identity"), "expected-build").await?);
        let mut child = fixture
            .spawn(root.path(), &socket, &response(protocol, identity, build)?)
            .await?;
        let result =
            installation::ready(&socket, Some("expected-identity"), "expected-build").await;
        if succeeds {
            assert!(result?);
            assert!(matches!(
                installation::prepare(root.path(), &socket, "expected-build").await?,
                installation::Preparation::Ready
            ));
        } else {
            assert!(result.is_err());
        }
        assert!(child.try_wait()?.is_none());
        assert_current_protocol(root.path())?;
        child.kill().await?;
        std::fs::remove_file(socket)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
async fn legacy_database(root: &Path) -> TestResult<sqlx::SqliteConnection> {
    use sqlx::Connection;
    let path = root.join("execution.sqlite3");
    paths::file(&path)?;
    let mut db = sqlx::SqliteConnection::connect_with(
        &sqlx::sqlite::SqliteConnectOptions::new().filename(path),
    )
    .await?;
    sqlx::raw_sql(include_str!("../src/storage/schema.sql"))
        .execute(&mut db)
        .await?;
    sqlx::raw_sql("INSERT INTO identity VALUES(1,'preserved-identity'); PRAGMA application_id=1380143958; PRAGMA user_version=2;")
        .execute(&mut db).await?;
    sqlx::query("INSERT INTO operations(id,digest,result) VALUES('completed','digest',?)")
        .bind(json!({"preserved":true}).to_string())
        .execute(&mut db)
        .await?;
    Ok(db)
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn idle_legacy_service_is_replaced_without_protocol_downgrade() -> TestResult {
    use sha2::{Digest, Sha256};
    let fixture = Fixture::build()?;
    let root = private_directory()?;
    let socket = paths::socket(root.path())?;
    let mut db = legacy_database(root.path()).await?;
    let rejected = serde_json::to_string(&Frame::Error(Fault::new(
        "PROTOCOL_MISMATCH",
        "old service",
    )))?;
    let mut old = fixture.spawn(root.path(), &socket, &rejected).await?;
    let binary = env!("CARGO_BIN_EXE_remote-codex-server");
    let build = format!("{:x}", Sha256::digest(std::fs::read(binary)?));
    let installation::Preparation::Start(identity) =
        installation::prepare(root.path(), &socket, &build).await?
    else {
        return Err("idle legacy service was not replaced".into());
    };
    assert_eq!(identity.as_deref(), Some("preserved-identity"));
    tokio::time::timeout(Duration::from_secs(3), old.wait()).await??;
    let mut new = Command::new(binary)
        .arg("--state")
        .arg(root.path())
        .arg("serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if installation::ready(&socket, identity.as_deref(), &build).await? {
                break;
            }
            if new.try_wait()?.is_some() {
                return Err("replacement exited".into());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA user_version")
            .fetch_one(&mut db)
            .await?,
        3
    );
    let evidence: String = sqlx::query_scalar("SELECT result FROM operations WHERE id='completed'")
        .fetch_one(&mut db)
        .await?;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&evidence)?,
        json!({"preserved":true})
    );
    assert_current_protocol(root.path())?;
    new.kill().await?;
    std::fs::remove_file(socket)?;
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn unsafe_or_busy_legacy_installations_are_not_stopped() -> TestResult {
    let fixture = Fixture::build()?;
    let rejected = serde_json::to_string(&Frame::Error(Fault::new(
        "PROTOCOL_MISMATCH",
        "old service",
    )))?;
    for case in ["channel", "job", "transfer", "schema", "other-root"] {
        let root = private_directory()?;
        let other = private_directory()?;
        let socket = paths::socket(root.path())?;
        let mut db = legacy_database(root.path()).await?;
        let mut transfer = None;
        match case {
            "channel" => {
                sqlx::raw_sql("INSERT INTO execution_channels(id,profile,revision,state) VALUES('active','profile',7,'running')").execute(&mut db).await?;
            }
            "job" => {
                sqlx::raw_sql("INSERT INTO jobs(id,process_id,kind,profile,channel,cwd,state) VALUES('active','1','command','profile','channel','/tmp','running')").execute(&mut db).await?;
            }
            "transfer" => {
                transfer = Some(
                    nix::fcntl::Flock::lock(
                        paths::file(&root.path().join("transfer.lock"))?,
                        nix::fcntl::FlockArg::LockExclusiveNonblock,
                    )
                    .map_err(|(_, error)| error)?,
                );
            }
            "schema" => {
                sqlx::raw_sql("PRAGMA user_version=99")
                    .execute(&mut db)
                    .await?;
            }
            _ => {}
        }
        let state = if case == "other-root" {
            other.path()
        } else {
            root.path()
        };
        let mut child = fixture.spawn(state, &socket, &rejected).await?;
        let error = installation::prepare(root.path(), &socket, "expected-build")
            .await
            .err()
            .ok_or("expected rejected update")?;
        let expected = match case {
            "schema" => "UNSUPPORTED_SCHEMA",
            "other-root" => "SERVICE_UPDATE",
            _ => "SERVICE_UPDATE_BUSY",
        };
        assert_eq!(error.code, expected, "{case}: {}", error.message);
        assert!(child.try_wait()?.is_none());
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT id FROM identity")
                .fetch_one(&mut db)
                .await?,
            "preserved-identity"
        );
        if matches!(case, "channel" | "job") {
            let state: String = if case == "channel" {
                sqlx::query_scalar("SELECT state FROM execution_channels WHERE id='active'")
                    .fetch_one(&mut db)
                    .await?
            } else {
                sqlx::query_scalar("SELECT state FROM jobs WHERE id='active'")
                    .fetch_one(&mut db)
                    .await?
            };
            assert_eq!(state, "running");
        }
        assert_current_protocol(state)?;
        child.kill().await?;
        std::fs::remove_file(socket)?;
        drop(transfer);
    }
    Ok(())
}
