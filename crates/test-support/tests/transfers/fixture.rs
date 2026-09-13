use remote_codex_client::application::{AddServer, Client, ClientOptions, ServiceBundle};
use remote_codex_test_support::ProbeResult;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Arc};
pub(super) struct Fixture {
    pub(super) root: tempfile::TempDir,
    pub(super) workspace: String,
    pub(super) options: ClientOptions,
}
impl Fixture {
    pub(super) async fn open() -> ProbeResult<(Self, Client)> {
        let root = tempfile::tempdir()?;
        let workspace = format!(
            "/tmp/remote-codex-acceptance-transfers-{}",
            uuid::Uuid::new_v4()
        );
        let bytes = std::fs::read(std::env::var("REMOTE_CODEX_SERVICE")?)?;
        let options = ClientOptions {
            data_dir: Some(root.path().join("state")),
            service: ServiceBundle {
                sha256: format!("{:x}", Sha256::digest(&bytes)),
                bytes: Arc::from(bytes),
            },
            ..Default::default()
        };
        let fixture = Self {
            root,
            workspace,
            options,
        };
        fixture
            .command(&format!("umask 077; mkdir -p {}/work", fixture.workspace))
            .await?;
        let client = Client::open(fixture.options.clone()).await?;
        let target = std::env::var("REMOTE_CODEX_ACCEPTANCE_TARGET")?;
        let port = std::env::var("REMOTE_CODEX_ACCEPTANCE_PORT")?.parse()?;
        let endpoint = remote_codex_client::connection::SshEndpoint::parse(&target, Some(port))?;
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(fixture.root.path().join("state/state.sqlite3")),
        )
        .await?;
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO server_revisions(id) VALUES(?)")
            .bind(&id)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO connections(id,name,endpoint) VALUES(?,'transfer-test',?)")
            .bind(&id)
            .bind(serde_json::to_string(&endpoint)?)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO server_access(server,service_root) VALUES(?,?)")
            .bind(id)
            .bind(format!("{}/service", fixture.workspace))
            .execute(&pool)
            .await?;
        pool.close().await;
        client
            .servers()
            .add(AddServer {
                name: "transfer-test".into(),
                address: target,
                port: Some(port),
                settings: vec![],
                identity: None,
                install_key: false,
                password: None,
            })
            .await?;
        Ok((fixture, client))
    }
    pub(super) async fn server_files(&self, client: &Client) -> ProbeResult<()> {
        assert!(client.workspaces().list().await?.is_empty());
        let files = client.servers().files("transfer-test", "/").await?;
        assert_eq!(files.path(), "/");
        assert!(
            files
                .list("")
                .await?
                .entries
                .iter()
                .any(|e| e.name == "tmp")
        );
        assert!(client.workspaces().list().await?.is_empty());
        let parent = self.work().trim_start_matches('/').to_owned();
        let directory = format!("{parent}/file-context");
        files.create_directory(&directory).await?;
        let path = format!("{directory}/note.txt");
        let revision = files.write(&path, "root context", None).await?;
        let workspace = client
            .workspaces()
            .open("transfer-test", &self.work())
            .await?;
        assert_eq!(
            workspace.files().read("file-context/note.txt").await?.text,
            "root context"
        );
        workspace
            .files()
            .write(
                "file-context/note.txt",
                "workspace context",
                Some(revision.clone()),
            )
            .await?;
        assert!(files.write(&path, "stale", Some(revision)).await.is_err());
        assert_eq!(files.read(&path).await?.text, "workspace context");
        workspace.close();
        client
            .workspaces()
            .remove("transfer-test", &self.work())
            .await?;
        files
            .rename(&path, &format!("{directory}/renamed.txt"))
            .await?;
        files.remove(&format!("{directory}/renamed.txt")).await?;
        files.remove(&directory).await?;
        for invalid in ["", ".", "../escape", "/"] {
            assert!(files.remove(invalid).await.is_err());
        }
        assert!(client.workspaces().list().await?.is_empty());
        files.close();
        assert!(files.list("").await.is_err());
        Ok(())
    }
    pub(super) fn work(&self) -> String {
        format!("{}/work", self.workspace)
    }
    pub(super) async fn command(&self, script: &str) -> ProbeResult<String> {
        use tokio::io::AsyncWriteExt;
        let mut child = tokio::process::Command::new("/usr/bin/ssh")
            .args([
                "-S",
                &std::env::var("REMOTE_CODEX_ACCEPTANCE_CONTROL")?,
                "-p",
                &std::env::var("REMOTE_CODEX_ACCEPTANCE_PORT")?,
                "-o",
                "BatchMode=yes",
                "-o",
                "StrictHostKeyChecking=yes",
                "--",
                &std::env::var("REMOTE_CODEX_ACCEPTANCE_TARGET")?,
                "sh -s",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        let mut input = child.stdin.take().ok_or("SSH input missing")?;
        input.write_all(script.as_bytes()).await?;
        input.shutdown().await?;
        drop(input);
        let result = child.wait_with_output().await?;
        if !result.status.success() {
            return Err("SSH fixture operation failed".into());
        }
        Ok(String::from_utf8(result.stdout)?)
    }
    pub(super) async fn stop_service(&self) -> ProbeResult<()> {
        self.command(&format!(
            "RC_ROOT={}/service\n{}",
            self.workspace,
            include_str!("../../src/acceptance/stop-service.sh")
        ))
        .await?;
        Ok(())
    }
    pub(super) async fn cleanup(&self) -> ProbeResult<()> {
        self.stop_service().await?;
        self.command(&format!("rm -rf -- {}", self.workspace))
            .await?;
        Ok(())
    }
    pub(super) fn output(&self) -> ProbeResult<PathBuf> {
        let path = self.root.path().join("download");
        std::fs::create_dir(&path)?;
        Ok(path)
    }
}
pub(super) fn cut_relays() -> ProbeResult<()> {
    let log = std::fs::read_to_string(std::env::var("REMOTE_CODEX_ACCEPTANCE_RELAYS")?)?;
    for line in log.lines() {
        let value: serde_json::Value = serde_json::from_str(line)?;
        if value["kind"].is_null()
            && value["parent"].as_u64() == Some(u64::from(std::process::id()))
        {
            let pid = value["pid"].as_i64().ok_or("relay PID missing")? as i32;
            let owner = std::process::Command::new("ps")
                .args(["-o", "ppid=", "-p", &pid.to_string()])
                .output()?;
            if String::from_utf8(owner.stdout)?.trim() == std::process::id().to_string() {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
        }
    }
    Ok(())
}
