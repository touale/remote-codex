use super::{Context, headless};
use portable_pty::CommandBuilder;
use remote_codex_client::session::{PermissionPreset, SessionSettings};
use remote_codex_test_support::{
    ProbeResult,
    model::{function, message},
    terminal::Terminal,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::Notify,
};

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let session = headless::open(context, None, false).await?;
    session
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    let gateway = session.terminal(&[]).await?;
    let native = gateway.command.as_std();
    let args: Vec<_> = native.get_args().collect();
    let endpoint = args
        .windows(2)
        .find(|pair| pair[0] == "--remote")
        .and_then(|pair| pair[1].to_str())
        .ok_or("frontend endpoint missing")?;
    let upstream = endpoint
        .strip_prefix("unix://")
        .ok_or("frontend is not a Unix socket")?
        .to_owned();
    let temporary = tempfile::Builder::new()
        .prefix("rc-ui-test-")
        .tempdir_in("/tmp")?;
    let socket = temporary.path().join("proxy.sock");
    let listener = UnixListener::bind(&socket)?;
    let cut = Arc::new(Notify::new());
    let restored = Arc::new(Notify::new());
    let mut command = CommandBuilder::new(native.get_program());
    for arg in args {
        if arg == endpoint {
            command.arg(format!("unix://{}", socket.display()));
        } else {
            command.arg(arg);
        }
    }
    for (key, value) in native.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        }
    }
    if let Some(directory) = native.get_current_dir() {
        command.cwd(directory);
    }
    command.env("TERM", "xterm-256color");
    let mut terminal = Terminal::spawn(command)?;
    let relay = {
        let cut = cut.clone();
        let restored = restored.clone();
        tokio::spawn(async move {
            let (mut first, _) = listener.accept().await?;
            let mut backend = UnixStream::connect(&upstream).await?;
            tokio::select! {
                _ = cut.notified() => {},
                result = tokio::io::copy_bidirectional(&mut first, &mut backend) => { result?; },
            }
            drop(first);
            drop(backend);
            let (mut next, _) = listener.accept().await?;
            let mut backend = UnixStream::connect(&upstream).await?;
            restored.notify_one();
            tokio::io::copy_bidirectional(&mut next, &mut backend).await?;
            Ok::<_, std::io::Error>(())
        })
    };
    let result: ProbeResult<()> = async {
        terminal.wait_for("OpenAI Codex").await?;
        let before = context.model.requests()?.len();
        context.model.script([
            function(
                "exec_command",
                json!({
                    "cmd":"printf once >> frontend-recovery-once; sleep 5",
                    "workdir":context.environment.workspace, "yield_time_ms":10000
                }),
                None,
            ),
            message("Local frontend recovery completed."),
        ])?;
        terminal
            .type_command("Run the local frontend recovery task.")
            .await?;
        tokio::time::timeout(Duration::from_secs(20), async {
            while context.environment.absent("frontend-recovery-once").await? {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await??;
        cut.notify_one();
        tokio::time::timeout(Duration::from_secs(15), restored.notified()).await?;
        terminal
            .wait_for("Local frontend recovery completed.")
            .await?;
        if context.model.requests()?.len() != before + 2
            || context.environment.read("frontend-recovery-once").await? != "once"
            || session.snapshot()?.closed
        {
            return Err(
                "local frontend reconnect restarted or duplicated the running session".into(),
            );
        }
        terminal.type_command("/quit").await?;
        if terminal.wait_exit().await? != Some(0) {
            return Err("reconnected native frontend did not exit normally".into());
        }
        Ok(())
    }
    .await;
    let diagnostics = std::fs::write(
        context
            .environment
            .args
            .output
            .join("frontend-recovery.txt"),
        terminal.diagnostics(),
    );
    drop(terminal);
    relay.abort();
    let _ = relay.await;
    let finished = gateway.finish().await;
    session.close().await;
    result?;
    finished?;
    diagnostics?;
    eprintln!(
        "Frontend recovery: native TUI reconnected during execution; same session and exactly-once command; normal exit"
    );
    Ok(())
}
