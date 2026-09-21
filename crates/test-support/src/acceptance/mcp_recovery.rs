use super::{Context, headless, outage::Offline, tui};
use remote_codex_client::application::OpenSession;
use remote_codex_client::session::{
    EnvironmentState, PermissionPreset, SessionEvent, SessionSettings, TurnOutcome,
};
use remote_codex_test_support::{
    ProbeResult,
    model::{MARKER, function, message},
};
use serde_json::json;
use std::{path::Path, time::Duration};

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let config = context.environment.home.join("config.toml");
    let original = std::fs::read_to_string(&config)?;
    let fixture = context.environment.home.join("recovery-mcp.sh");
    std::fs::write(&fixture, include_str!("../../fixtures/mcp.sh"))?;
    let session = headless::open(context, None, false).await?;
    let id = session.session().id.clone();
    let result = async {
        session.settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        }).await?;
        // Twelve MB exceeds the eight-MiB replay buffer. Keep the command in the
        // foreground and allow its output so the fixture exercises replay loss.
        let command = r"printf once >> mcp-replay-once
while ! test -e mcp-replay-go; do sleep 0.1; done
head -c 12000000 /dev/zero | tr '\000' x
printf ready > mcp-replay-ready";
        context.model.script([function("exec_command", json!({
            "cmd": command,
            "workdir": context.environment.workspace,
            "yield_time_ms": 30000,
            "max_output_tokens": 4000000
        }), None)])?;
        let mut events = session.events();
        let mut states = session.environment();
        let turn = session.message(
            "Run the isolated recovery task.".into(),
            uuid::Uuid::new_v4().to_string(),
            None,
        ).await?.turn_id;
        tokio::time::timeout(Duration::from_secs(20), async {
            while context.environment.absent("mcp-replay-once").await? {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }).await??;
        let offline = Offline::start(std::process::id()).await?;
        tokio::time::timeout(Duration::from_secs(20), async {
            while matches!(*states.borrow(), EnvironmentState::Ready) {
                states.changed().await?;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }).await??;
        // Produce more than the bounded replay buffer while the relay is offline.
        context.environment.write(
            &format!("{}/mcp-replay-go", context.environment.workspace), "go",
        ).await?;
        tokio::time::timeout(Duration::from_secs(45), async {
            while context.environment.absent("mcp-replay-ready").await? {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }).await??;
        std::fs::write(&config, with_probe(&original, &fixture, "RECOVERED")?)?;
        drop(offline);
        let mut continued = 0;
        let mut expired = false;
        let mut completed_before_rebuild = false;
        tokio::time::timeout(Duration::from_secs(90), async {
            loop {
                if expired && matches!(*states.borrow(), EnvironmentState::Ready)
                    && session.snapshot()?.turn.is_none()
                {
                    break;
                }
                tokio::select! {
                    event = events.recv() => match event? {
                        SessionEvent::EnvironmentChanged { state: EnvironmentState::Recovering { reason } } => {
                            eprintln!("MCP recovery: {reason}");
                            expired |= reason.contains("execution replay");
                        },
                        SessionEvent::TurnStarted { id: next, .. } if next != turn => continued += 1,
                        SessionEvent::TurnCompleted { id: next, outcome: TurnOutcome::Completed, .. } if next == turn => completed_before_rebuild = true,
                        SessionEvent::Closed { reason } => return Err(format!("MCP recovery closed session: {reason:?}").into()),
                        _ => {},
                    },
                    changed = states.changed() => {
                        changed?;
                        if let EnvironmentState::ActionRequired { message, .. } = states.borrow().clone() {
                            return Err(message.into());
                        }
                    }
                }
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }).await.map_err(|_| format!(
            "replay recovery did not become ready and idle: expired={expired}, continued={continued}, state={:?}",
            *states.borrow(),
        ))??;
        if continued != usize::from(!completed_before_rebuild) || session.session().id != id
            || context.environment.read("mcp-replay-once").await? != "once"
        {
            return Err("replay recovery did not preserve identity and exactly-once execution".into());
        }
        let settings = session.current_settings()?;
        if !settings.full_access || settings.approval_policy != "never" {
            return Err("MCP recovery lost confirmed permissions".into());
        }
        probe(context, &session, "RECOVERED").await?;

        // Prepare while valid, then edit the file before opening. An invalid
        // takeover must fail before it revokes the existing session's lease.
        let takeover = context.client.sessions().prepare(OpenSession {
            server: "test".into(),
            path: context.environment.workspace.clone(),
            resume: Some(id.clone()),
            takeover: true,
            mcp_source: None,
        }).await?;
        std::fs::write(&config, "[mcp_servers")?;
        let error = takeover.open(false).await.err()
            .ok_or("invalid MCP takeover succeeded")?;
        if error.code() != "MCP_CONFIGURATION_INVALID" || session.snapshot()?.closed {
            return Err("invalid MCP configuration revoked the existing session".into());
        }
        std::fs::write(&config, with_probe(&original, &fixture, "RECOVERED")?)?;
        probe(context, &session, "RECOVERED").await?;

        // A malformed local edit is repairable without closing the App session.
        std::fs::write(&config, "[mcp_servers")?;
        context.environment.stop_service().await?;
        tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                if let EnvironmentState::ActionRequired { code, message } = states.borrow().clone() {
                    if code != "MCP_CONFIGURATION_INVALID" {
                        return Err(message.into());
                    }
                    break;
                }
                states.changed().await?;
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }).await??;
        std::fs::write(&config, with_probe(&original, &fixture, "REPAIRED")?)?;
        // The same message entry point used by the App resumes and submits once.
        probe(context, &session, "REPAIRED").await?;
        if session.snapshot()?.closed || session.session().id != id {
            return Err("repair required reopening the session".into());
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }.await;
    session.close().await;
    std::fs::write(config, original)?;
    result?;
    eprintln!(
        "MCP recovery: replay restored; invalid takeover preserved the session; configuration repaired in place"
    );
    terminal(context).await
}

async fn probe(
    context: &Context,
    session: &remote_codex_client::application::SessionHandle,
    marker: &str,
) -> ProbeResult<()> {
    let call = function("probe_identity", json!({}), Some("mcp__recovery_probe"));
    let call_id = call["call_id"].clone();
    context.model.script([
        json!({
            "type":"tool_search_call", "id":"recovery_search", "call_id":"recovery_search",
            "execution":"client", "arguments":{"query":"recovery_probe probe_identity","limit":1},
            "status":"completed"
        }),
        call,
        message("MCP recovery checked."),
    ])?;
    headless::turn(session, false).await?;
    let requests = context.model.requests()?;
    let input = requests
        .last()
        .and_then(|r| r["input"].as_array())
        .ok_or("model input missing")?;
    let output = input
        .iter()
        .find(|v| v["type"] == "function_call_output" && v["call_id"] == call_id)
        .ok_or("MCP output missing")?;
    if !output.to_string().contains(&format!("MARKER={marker}")) {
        return Err("MCP did not use the latest local settings".into());
    }
    Ok(())
}

async fn terminal(context: &Context) -> ProbeResult<()> {
    let mut terminal = tui::start(context, &[])?;
    terminal.wait_for("OpenAI Codex").await?;
    terminal
        .type_command("Initialize the MCP recovery session.")
        .await?;
    terminal.wait_for(MARKER).await?;
    let config = context.environment.home.join("config.toml");
    let original = std::fs::read_to_string(&config)?;
    std::fs::write(&config, "[mcp_servers")?;
    let result = async {
        context.environment.stop_service().await?;
        terminal.wait_for("MCP_CONFIGURATION_INVALID").await?;
        std::fs::write(&config, &original)?;
        let before = context.model.requests()?.len();
        terminal
            .type_command("Reconnect after fixing local MCP.")
            .await?;
        terminal
            .wait_for("Execution connection restored. Send your message again.")
            .await?;
        if context.model.requests()?.len() != before {
            return Err("CLI submitted the recovery trigger as a user message".into());
        }
        terminal
            .type_command("Continue in this same terminal.")
            .await?;
        context.model.wait_for(before + 1).await?;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;
    std::fs::write(config, original)?;
    terminal.type_command("/quit").await?;
    let status = terminal.wait_exit().await?;
    std::fs::write(
        context.environment.args.output.join("tui-mcp-recovery.txt"),
        terminal.diagnostics(),
    )?;
    result?;
    if status != Some(0) {
        return Err("MCP recovery terminal did not exit normally".into());
    }
    eprintln!(
        "TUI MCP recovery: fixed configuration retried in the same frontend without submitting the trigger"
    );
    Ok(())
}

fn with_probe(original: &str, fixture: &Path, marker: &str) -> ProbeResult<String> {
    Ok(format!(
        "{original}\n{}",
        toml::to_string(&json!({"mcp_servers":{"recovery_probe": {
            "command":"/bin/sh", "args":[fixture], "required":true, "env":{"PROBE_MARKER":marker}
        }}}))?
    ))
}
