use crate::{args::Cli, context::Context, output, selection, ui};
use remote_codex_client::{ClientError, Result, application::OpenSession};

pub(crate) async fn start(cli: &Cli, context: &Context) -> Result<()> {
    interactive(cli)?;
    let server = selection::server(cli, context).await?;
    let path = selection::workspace(cli, &context.client, &server).await?;
    open(
        cli,
        context,
        OpenSession {
            server: server.name,
            path,
            resume: None,
            takeover: false,
            mcp_source: cli.mcp_source.clone(),
        },
    )
    .await
}

pub(crate) async fn resume(
    cli: &Cli,
    context: &Context,
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
    let sessions = context.client.sessions();
    let cached = if let Some(id) = id {
        sessions.resolve(id, cli.connection.as_deref()).await?
    } else {
        let mut records = sessions.list(cli.connection.as_deref(), archived).await?;
        if cli.json {
            return output::json(&serde_json::json!({"sessions":records}));
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
        if records.is_empty() {
            println!("No local sessions. Start one with remote-codex -n NAME.");
            return Ok(());
        }
        let index = ui::choose("Local sessions — Enter to resume", &labels)?;
        records.remove(index)
    };
    if read {
        return output::json(&sessions.read(&cached.session.id, cursor).await?);
    }
    interactive(cli)?;
    open(
        cli,
        context,
        OpenSession {
            server: cached.server,
            path: cached.session.cwd,
            resume: Some(cached.session.id),
            takeover: cli.takeover,
            mcp_source: cli.mcp_source.clone(),
        },
    )
    .await
}

async fn open(cli: &Cli, context: &Context, options: OpenSession) -> Result<()> {
    let prepared = context
        .prepare(context.client.sessions().prepare(options))
        .await?;
    let trust = prepared.trust();
    let approved = cli.trust_project_mcp
        || (trust.required
            && ui::confirm(
                &format!(
                    "Allow project MCP servers {} to run on {}? Review {}/.codex/config.toml before trusting.",
                    ui::text(&trust.names.join(", ")),
                    ui::text(&trust.server),
                    ui::text(&trust.path)
                ),
                false,
            )?);
    let handle = context.prepare(prepared.open(approved)).await?;
    crate::frontend::run(cli, handle, &cli.native_arguments).await
}

fn interactive(cli: &Cli) -> Result<()> {
    if cli.json || !ui::interactive() {
        return Err(ClientError::Argument(
            "chat requires an interactive terminal; use resume --all --json to list local sessions",
        ));
    }
    Ok(())
}
