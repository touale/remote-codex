use portable_pty::CommandBuilder;
use remote_codex_client::{local::history, store::LocalStore};
use remote_codex_test_support::{ProbeResult, terminal::Terminal};
use std::{path::PathBuf, time::Duration};

async fn ready(terminal: &Terminal) -> ProbeResult<()> {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !terminal.diagnostics().contains("m0-probe") {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| {
        format!(
            "TUI did not load the fixture session: {}",
            terminal.diagnostics()
        )
    })?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires an explicitly configured SSH test profile using the isolated m0-probe model fixture"]
async fn official_tui_creates_and_resumes_the_same_local_session() -> ProbeResult<()> {
    let state = PathBuf::from(std::env::var("REMOTE_CODEX_TEST_STATE")?).canonicalize()?;
    let name = std::env::var("REMOTE_CODEX_TEST_SERVER")?;
    let workspace = std::env::var("REMOTE_CODEX_TEST_WORKSPACE")?;
    let program = std::env::var_os("REMOTE_CODEX_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_remote-codex")));
    let store = LocalStore::open(&state).await?;
    let server = store.find_connection(&name).await?;
    let before = store.cached_sessions(Some(&server.id), false).await?;
    let mut command = CommandBuilder::new(&program);
    command.args([
        "--data-dir",
        state.to_str().ok_or("invalid path")?,
        "-n",
        &name,
        "--path",
        &workspace,
    ]);
    command.env("TERM", "xterm-256color");
    let mut terminal = Terminal::spawn(command)?;
    ready(&terminal).await?;
    terminal
        .type_command("Reply with the fixture marker. Do not use tools.")
        .await?;
    let created = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(session) = store
                .cached_sessions(Some(&server.id), false)
                .await?
                .into_iter()
                .find(|s| !before.iter().any(|old| old.session.id == s.session.id))
            {
                return Ok::<_, remote_codex_client::ClientError>(session);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await??;
    let codex = remote_codex_client::runtime::tui::program(&state, |_| {}).await?;
    let binding = store.session_binding(&created.session.id).await?;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let history = history::read(&codex, &binding, None).await;
            if history.is_ok_and(|value| value.to_string().contains("M0_LOCAL_FIXTURE")) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| format!("no persisted model response: {}", terminal.diagnostics()))?;
    terminal.type_command("/quit").await?;
    assert_eq!(
        terminal.wait_exit().await?,
        Some(0),
        "{}",
        terminal.diagnostics()
    );
    let mut command = CommandBuilder::new(&program);
    command.args([
        "--data-dir",
        state.to_str().ok_or("invalid path")?,
        "resume",
        &created.session.id,
        "-n",
        &name,
    ]);
    command.env("TERM", "xterm-256color");
    let mut resumed = Terminal::spawn(command)?;
    ready(&resumed).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        resumed.diagnostics().contains("M0_LOCAL_FIXTURE"),
        "{}",
        resumed.diagnostics()
    );
    resumed.type_command("/quit").await?;
    assert_eq!(resumed.wait_exit().await?, Some(0));
    let history = history::read(&codex, &binding, None).await?;
    assert_eq!(
        history["thread"]["turns"]
            .as_array()
            .ok_or("turns missing")?
            .len(),
        1,
        "resume must not replay the prompt"
    );
    assert_eq!(history["thread"]["cwd"], workspace);
    assert!(history["thread"]["forkedFromId"].is_null());
    store.close().await;
    Ok(())
}
