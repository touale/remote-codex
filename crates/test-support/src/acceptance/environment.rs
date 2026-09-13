use super::Arguments;
use remote_codex_client::application::{AddServer, Client};
use remote_codex_test_support::ProbeResult;
use std::{path::PathBuf, process::Stdio};
use tokio::io::AsyncWriteExt;

pub(super) struct Environment {
    pub(super) args: Arguments,
    pub(super) state: PathBuf,
    pub(super) home: PathBuf,
    pub(super) workspace: String,
}

impl Environment {
    pub(super) async fn prepare(mut args: Arguments) -> ProbeResult<Self> {
        for path in [&args.cli, &args.service] {
            if !path.is_file() {
                return Err(format!("required file missing: {}", path.display()).into());
            }
        }
        args.cli = args.cli.canonicalize()?;
        args.service = args.service.canonicalize()?;
        args.output = std::path::absolute(&args.output)?;
        remote_codex_adapter::program::discover().await?;
        std::fs::create_dir_all(&args.output)?;
        let root = PathBuf::from(
            std::env::var_os("REMOTE_CODEX_ACCEPTANCE_ROOT").ok_or("test root missing")?,
        );
        let home =
            PathBuf::from(std::env::var_os("CODEX_HOME").ok_or("isolated Codex home missing")?);
        std::fs::create_dir_all(&home)?;
        let workspace = format!("/tmp/remote-codex-acceptance-{}", uuid::Uuid::new_v4());
        let fixture = Self {
            args,
            state: root.join("state"),
            home,
            workspace,
        };
        fixture
            .command(&format!(
                "umask 077; mkdir -p {}",
                quote(&fixture.workspace)?
            ))
            .await?;
        Ok(fixture)
    }

    pub(super) async fn configure(&self, base_url: &str) -> ProbeResult<()> {
        let config = format!(
            "developer_instructions=\"LOCAL_INSTRUCTIONS_MARKER\"\nmodel=\"fixture-model\"\nmodel_provider=\"fixture\"\n[model_providers.fixture]\nname=\"Isolated Responses\"\nbase_url={}\nwire_api=\"responses\"\nrequires_openai_auth=false\n[analytics]\nenabled=false\n[projects.{}]\ntrust_level=\"trusted\"\n",
            serde_json::to_string(base_url)?,
            serde_json::to_string(&self.workspace)?
        );
        std::fs::write(self.home.join("config.toml"), config)?;
        Ok(())
    }

    pub(super) async fn register(&self, client: &Client) -> ProbeResult<()> {
        let mut settings = vec![
            ("execution.mode".into(), "sandboxed".into()),
            ("background".into(), "true".into()),
        ];
        if let Some(proxy) = &self.args.proxy {
            settings.extend([
                ("proxy.mode".into(), "custom".into()),
                ("env.http_proxy".into(), proxy.clone()),
                ("env.https_proxy".into(), proxy.clone()),
            ]);
        }
        let server = client
            .servers()
            .add(AddServer {
                password: None,
                name: "test".into(),
                address: self.args.target.clone(),
                port: Some(self.args.port),
                settings,
                identity: self.args.identity.clone(),
                install_key: false,
            })
            .await?;
        // Fault injection owns a private supervisor, never the user's active service.
        let pool = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(self.state.join("state.sqlite3")),
        )
        .await?;
        sqlx::query("UPDATE server_access SET service_root=?,service_executable=NULL,remote_identity=NULL,applied_revision=NULL WHERE server=?")
            .bind(format!("{}/service", self.workspace)).bind(&server.id).execute(&pool).await?;
        // Model interrupted preparation; the next ordinary session must finish it.
        sqlx::query("UPDATE connections SET runtime=NULL WHERE id=?")
            .bind(&server.id)
            .execute(&pool)
            .await?;
        pool.close().await;
        Ok(())
    }

    pub(super) async fn command(&self, script: &str) -> ProbeResult<String> {
        let mut command = tokio::process::Command::new("/usr/bin/ssh");
        if let Some(config) = &self.args.ssh_config {
            command.arg("-F").arg(config);
        }
        if let Some(control) = &self.args.control {
            command.arg("-S").arg(control);
        }
        if let Some(identity) = &self.args.identity {
            command.arg("-i").arg(identity);
        }
        command
            .args([
                "-o",
                "BatchMode=yes",
                "-o",
                "IdentitiesOnly=yes",
                "-o",
                "ForwardAgent=no",
                "-o",
                "StrictHostKeyChecking=yes",
            ])
            .arg("-p")
            .arg(self.args.port.to_string())
            .arg("--")
            .arg(&self.args.target)
            .arg("sh -s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        let mut child = command.spawn()?;
        let mut input = child.stdin.take().ok_or("SSH input missing")?;
        input.write_all(script.as_bytes()).await?;
        input.shutdown().await?;
        drop(input);
        let output =
            tokio::time::timeout(std::time::Duration::from_secs(30), child.wait_with_output())
                .await??;
        if !output.status.success() {
            return Err(format!("fixture SSH failed: {}", output.status).into());
        }
        Ok(String::from_utf8(output.stdout)?)
    }

    pub(super) async fn write(&self, path: &str, content: &str) -> ProbeResult<()> {
        if !path.starts_with(&format!("{}/", self.workspace)) {
            return Err("write outside fixture workspace".into());
        }
        let parent = std::path::Path::new(path)
            .parent()
            .and_then(|path| path.to_str())
            .ok_or("invalid fixture path")?;
        self.command(&format!(
            "umask 077; mkdir -p {}; printf '%s' {} > {}",
            quote(parent)?,
            quote(content)?,
            quote(path)?
        ))
        .await?;
        Ok(())
    }

    pub(super) async fn read(&self, name: &str) -> ProbeResult<String> {
        self.command(&format!(
            "cat -- {}",
            quote(&format!("{}/{name}", self.workspace))?
        ))
        .await
    }

    pub(super) async fn absent(&self, name: &str) -> ProbeResult<bool> {
        Ok(self
            .command(&format!(
                "if test -e {}; then printf exists; fi",
                quote(&format!("{}/{name}", self.workspace))?
            ))
            .await?
            .is_empty())
    }

    pub(super) async fn cleanup(&self) -> ProbeResult<()> {
        if !self.workspace.starts_with("/tmp/remote-codex-acceptance-") {
            return Err("invalid cleanup scope".into());
        }
        self.stop_service().await?;
        self.command(&format!("rm -rf -- {}", quote(&self.workspace)?))
            .await?;
        Ok(())
    }

    pub(super) async fn stop_service(&self) -> ProbeResult<()> {
        self.command(&format!(
            "RC_ROOT={}\n{}",
            quote(&format!("{}/service", self.workspace))?,
            include_str!("stop-service.sh")
        ))
        .await?;
        Ok(())
    }
}

pub(super) fn quote(value: &str) -> ProbeResult<String> {
    Ok(shlex::try_quote(value)?.into_owned())
}
