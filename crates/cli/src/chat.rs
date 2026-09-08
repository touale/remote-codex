use crate::{args::Cli, output, selection, ui};
use remote_codex_client::{
    ClientError, Result, local::OpenOptions, runtime, sessions, store::LocalStore,
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) async fn start(
    cli: &Cli,
    store: &LocalStore,
    directory: &Path,
    arguments: &[OsString],
) -> Result<()> {
    interactive(cli)?;
    let server = selection::server(cli, store, directory).await?;
    let path = selection::workspace(cli, store, &server).await?;
    let program = program(cli, directory).await?;
    let remote = Arc::new(crate::connection::connect(cli, store, directory, server).await?);
    remote.synchronize(store).await?;
    let mcp = mcp(cli, store, &remote, &path).await?;
    let runtime = crate::chat_prepare::open(
        cli,
        store,
        remote,
        OpenOptions {
            directory,
            program: &program,
            path: &path,
            existing: None,
            takeover: false,
            mcp,
            progress: None,
        },
    )
    .await?;
    crate::frontend::run(cli, &program, runtime, arguments).await
}

pub(crate) async fn resume(
    cli: &Cli,
    store: &LocalStore,
    directory: &Path,
    id: Option<&str>,
    archived: bool,
    read: bool,
    cursor: Option<&str>,
) -> Result<()> {
    if cli.path.is_some() {
        return Err(ClientError::Argument(
            "resume preserves the bound remote directory; --path is for new sessions",
        ));
    }
    let selected = match &cli.connection {
        Some(name) => Some(store.find_connection(name).await?.id),
        None => None,
    };
    let cached = if let Some(id) = id {
        sessions::resolve(store, id, cli.connection.as_deref()).await?
    } else {
        let mut records = store.cached_sessions(selected.as_deref(), archived).await?;
        if cli.json {
            return output::json(&serde_json::json!({"schema_version":3,"sessions":records}));
        }
        let labels: Vec<_> = records
            .iter()
            .map(|s| format!("{} | {} | {}", s.server, s.session.cwd, s.session.title))
            .collect();
        if !ui::interactive() {
            for (record, label) in records.iter().zip(labels) {
                println!("{}  {}", ui::text(&record.session.id), ui::text(&label));
            }
            return Ok(());
        }
        let index = ui::choose("Local sessions — Enter to resume", &labels)?;
        records.remove(index)
    };
    let binding = store.session_binding(&cached.session.id).await?;
    let program = program(cli, directory).await?;
    if read {
        return output::json(
            &remote_codex_client::local::history::read(&program, &binding, cursor).await?,
        );
    }
    interactive(cli)?;
    let server = store.connection_by_id(&cached.server_id).await?;
    let remote = Arc::new(crate::connection::connect(cli, store, directory, server).await?);
    remote.synchronize(store).await?;
    let path = binding.session.cwd.clone();
    let mcp = mcp(cli, store, &remote, &path).await?;
    let runtime = crate::chat_prepare::open(
        cli,
        store,
        remote,
        OpenOptions {
            directory,
            program: &program,
            path: &path,
            existing: Some(binding),
            takeover: cli.takeover,
            mcp,
            progress: None,
        },
    )
    .await?;
    crate::frontend::run(cli, &program, runtime, &[]).await
}

fn interactive(cli: &Cli) -> Result<()> {
    if cli.json || !ui::interactive() {
        return Err(ClientError::Argument(
            "chat requires an interactive terminal; use resume --all --json to list local sessions",
        ));
    }
    Ok(())
}

async fn program(cli: &Cli, directory: &Path) -> Result<PathBuf> {
    let display = std::cell::RefCell::new(crate::progress::PrepareProgress::stderr(cli.json));
    let operation = runtime::tui::program(directory, |event| {
        display.borrow_mut().event(event, std::time::Instant::now())
    });
    tokio::pin!(operation);
    let mut timer = tokio::time::interval(std::time::Duration::from_millis(200));
    let result = loop {
        tokio::select! {result=&mut operation=>break result,_=timer.tick()=>display.borrow_mut().tick(std::time::Instant::now())}
    };
    display.borrow_mut().finish(std::time::Instant::now());
    result
}

async fn mcp(
    cli: &Cli,
    store: &LocalStore,
    remote: &remote_codex_client::remote::Remote,
    path: &str,
) -> Result<remote_codex_client::extensions::mcp::McpPlan> {
    let mut project = remote_codex_client::extensions::mcp::inspect(store, remote, path).await?;
    if !project.names.is_empty()
        && !project.trusted
        && (cli.trust_project_mcp
            || (ui::interactive()
                && ui::confirm(
                    &format!(
                        "Allow project MCP servers {} to run on {}? Review {}/.codex/config.toml before trusting.",
                        ui::text(&project.names.join(", ")),
                        ui::text(&remote.server.name),
                        ui::text(&project.path)
                    ),
                    false,
                )?))
    {
        project.trust(store, &remote.server.id).await?;
    }
    project.resolve(
        &remote_codex_client::local::codex_home()?,
        &format!("rc_{}", remote.server.id.replace('-', "")),
        cli.mcp_source.as_deref(),
    )
}
