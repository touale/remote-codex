mod environment;
mod extensions;
mod frontend_recovery;
mod goals;
mod headless;
mod lifecycle;
mod mcp_recovery;
mod outage;
mod password_recovery;
mod reconnect;
mod restart;
mod saved_password;
mod ssh;
mod tui;
mod tui_edit;
mod tui_goal;
mod tui_outage;
mod tui_recovery;
mod tui_resume;

use clap::Parser;
use environment::Environment;
use remote_codex_client::application::{Client, ClientOptions, ServiceBundle};
use remote_codex_test_support::{ProbeResult, model::ModelFixture};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Parser)]
pub(super) struct Arguments {
    #[arg(long, env = "REMOTE_CODEX_NATIVE")]
    codex: PathBuf,
    #[arg(long, env = "REMOTE_CODEX_CLI")]
    cli: PathBuf,
    #[arg(long, env = "REMOTE_CODEX_SERVICE")]
    service: PathBuf,
    #[arg(long, env = "REMOTE_CODEX_SSH_TARGET")]
    target: String,
    #[arg(long, env = "REMOTE_CODEX_SSH_PORT")]
    port: u16,
    #[arg(long, env = "REMOTE_CODEX_SSH_IDENTITY")]
    identity: Option<PathBuf>,
    /// Reuse an authenticated SSH master; does not install remote credentials.
    #[arg(long)]
    control: Option<PathBuf>,
    #[arg(long, env = "REMOTE_CODEX_SSH_CONFIG")]
    ssh_config: Option<PathBuf>,
    #[arg(long)]
    proxy: Option<String>,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, hide = true)]
    worker: bool,
    /// Verify manual and saved-password recovery using a temporary Keychain entry, then remove it.
    #[arg(long)]
    password_recovery: bool,
    /// Run the saved-password contract alone for credential-backend diagnostics.
    #[arg(long)]
    credentials_only: bool,
    /// Run CLI and App recovery checks without repeating the full acceptance suite.
    #[arg(long)]
    recovery_only: bool,
    /// Verify native prompt editing and the resulting remote session bindings.
    #[arg(long)]
    editing_only: bool,
}

pub(super) struct Context {
    environment: Environment,
    client: Client,
    model: ModelFixture,
}

pub(super) async fn run() -> ProbeResult<()> {
    if std::env::args_os().next().is_some_and(|path| {
        std::path::Path::new(&path)
            .file_name()
            .is_some_and(|name| name == "ssh")
    }) {
        return ssh::relay().await;
    }
    let args = Arguments::parse();
    if !args.worker {
        std::fs::create_dir_all(&args.output)?;
        std::fs::write(
            args.output.join("report.json"),
            serde_json::to_vec_pretty(&serde_json::json!({"passed":false,"stage":"starting"}))?,
        )?;
        let root = tempfile::Builder::new()
            .prefix("rc-acceptance-")
            .tempdir_in("/tmp")?;
        let home = root.path().join("local");
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin)?;
        if args.password_recovery || args.credentials_only {
            // Keep Codex/Skills isolated while allowing the OS vault to locate
            // the logged-in user's Keychain. Only test-owned entries are changed.
            let user_home = std::env::var_os("HOME").ok_or("user home missing")?;
            let keychains = PathBuf::from(user_home)
                .join("Library/Keychains")
                .canonicalize()?;
            std::fs::create_dir_all(home.join("Library"))?;
            std::os::unix::fs::symlink(keychains, home.join("Library/Keychains"))?;
        }
        std::os::unix::fs::symlink(args.codex.canonicalize()?, bin.join("codex"))?;
        std::os::unix::fs::symlink(std::env::current_exe()?, bin.join("ssh"))?;
        let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
            &std::env::var_os("PATH").ok_or("PATH missing")?,
        )))?;
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command
            .args(std::env::args_os().skip(1))
            .arg("--worker")
            .env("HOME", &home)
            .env("CODEX_HOME", home.join(".codex"))
            .env("PATH", path)
            .env("REMOTE_CODEX_ACCEPTANCE_ROOT", root.path())
            .kill_on_drop(true);
        command
            .env("REMOTE_CODEX_ACCEPTANCE_TARGET", &args.target)
            .env("REMOTE_CODEX_ACCEPTANCE_PORT", args.port.to_string())
            .env(
                "REMOTE_CODEX_ACCEPTANCE_RELAYS",
                root.path().join("relays.jsonl"),
            );
        command.env(
            "REMOTE_CODEX_ACCEPTANCE_OUTAGE",
            root.path().join("offline"),
        );
        if let Some(control) = &args.control {
            command.env("REMOTE_CODEX_ACCEPTANCE_CONTROL", control);
        }
        let status = command.status().await?;
        if !status.success() {
            eprintln!(
                "Failed acceptance state retained for cleanup: {}",
                root.keep().display()
            );
            return Err("release acceptance failed; inspect the report directory".into());
        }
        return Ok(());
    }
    let environment = Environment::prepare(args).await?;
    let model = ModelFixture::start().await?;
    environment.configure(&model.base_url).await?;
    let bytes = std::fs::read(&environment.args.service)?;
    let synchronizations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = synchronizations.clone();
    let client = Client::open(ClientOptions {
        data_dir: Some(environment.state.clone()),
        ssh_config: environment.args.ssh_config.clone(),
        interactive: false,
        service: ServiceBundle {
            sha256: sha256(&bytes),
            bytes: Arc::from(bytes),
        },
        progress: Some(Arc::new(move |event| {
            if matches!(
                event,
                remote_codex_client::progress::PrepareEvent::Stage(
                    remote_codex_client::progress::PrepareStage::Synchronize
                )
            ) {
                observed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            eprintln!("prepare: {event:?}");
        })),
        notice: Some(Arc::new(|notice| eprintln!("notice: {}", notice.message()))),
        ..Default::default()
    })
    .await?;
    let context = Context {
        environment,
        client,
        model,
    };
    let result = async {
        context.environment.register(&context.client).await?;
        if synchronizations.load(std::sync::atomic::Ordering::Relaxed) != 1 {
            return Err("registration must synchronize configuration once".into());
        }
        if context.environment.args.credentials_only {
            let password = password_recovery::read_password()?;
            return saved_password::exercise(&context, &password).await;
        }
        let retried = headless::open(&context, None, false).await?;
        if synchronizations.load(std::sync::atomic::Ordering::Relaxed) != 2 {
            return Err(
                "retrying incomplete preparation must synchronize configuration once".into(),
            );
        }
        retried.close().await;
        if context.environment.args.editing_only {
            return tui_edit::exercise(&context).await;
        }
        if context.environment.args.recovery_only {
            frontend_recovery::exercise(&context).await?;
            mcp_recovery::exercise(&context).await?;
            goals::exercise(&context).await?;
            outage::messages(&context).await?;
            return tui_recovery::exercise(&context).await;
        }
        headless::exercise(&context).await?;
        tui_resume::exercise(&context).await?;
        extensions::exercise(&context).await?;
        lifecycle::exercise(&context).await?;
        reconnect::exercise(&context).await?;
        outage::exercise(&context).await?;
        restart::exercise(&context).await?;
        frontend_recovery::exercise(&context).await?;
        mcp_recovery::exercise(&context).await?;
        goals::exercise(&context).await?;
        tui_goal::exercise(&context).await?;
        tui::exercise(&context).await?;
        tui_edit::exercise(&context).await?;
        tui_recovery::exercise(&context).await?;
        if context.environment.args.password_recovery {
            password_recovery::exercise(&context).await?;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;
    context.client.close().await;
    let cleanup = context.environment.cleanup().await;
    std::fs::write(
        context.environment.args.output.join("requests.json"),
        serde_json::to_vec_pretty(&context.model.requests()?)?,
    )?;
    std::fs::write(
        context.environment.args.output.join("report.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "passed":result.is_ok() && cleanup.is_ok(),"transport":if context.environment.args.control.is_some() {"OpenSSH with reused authentication; local readiness socket fixture"} else {"OpenSSH with identity file"},"native_version":remote_codex_adapter::program::inspect(&remote_codex_adapter::program::discover().await?).await?.version,
        "headless_and_tui":!context.environment.args.credentials_only && !context.environment.args.recovery_only && !context.environment.args.editing_only,
        "recovery_only":context.environment.args.recovery_only,
        "editing_only":context.environment.args.editing_only,
        "password_recovery":context.environment.args.password_recovery && !context.environment.args.credentials_only,
        "saved_password":context.environment.args.password_recovery || context.environment.args.credentials_only,
        "host_reboot":"not_run",
        "cli_sha256":sha256(&std::fs::read(&context.environment.args.cli)?),
        "service_sha256":sha256(&std::fs::read(&context.environment.args.service)?),
        "error":result.as_ref().err().map(ToString::to_string),"cleanup_error":cleanup.as_ref().err().map(ToString::to_string)
        }))?,
    )?;
    result?;
    cleanup
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
