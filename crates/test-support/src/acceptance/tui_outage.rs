use super::{Context, outage::Offline};
use remote_codex_test_support::{ProbeResult, model::function, terminal::Terminal};
use serde_json::json;
use std::time::Duration;

pub(super) async fn exercise(context: &Context, terminal: &Terminal) -> ProbeResult<()> {
    context
        .client
        .config()
        .set("test", "reconnect.max_attempts", "1", false, None)
        .await?;
    context.model.script([function("exec_command", json!({"cmd":"printf once >> tui-outage-count; sleep 20", "workdir":context.environment.workspace,"yield_time_ms":10000}), None)])?;
    terminal.type_command("Run the fixed outage task.").await?;
    tokio::time::timeout(Duration::from_secs(20), async {
        while context.environment.absent("tui-outage-count").await? {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
    .await??;
    let offline = Offline::start(terminal.process_id()?).await?;
    terminal
        .wait_for("Connection failed after 1 attempt.")
        .await?;
    let requests = context.model.requests()?.len();
    context
        .client
        .config()
        .set("test", "reconnect.max_attempts", "2", false, None)
        .await?;
    terminal
        .type_command("Try this new message while offline.")
        .await?;
    // The native frontend must release input before the next five-second retry.
    tokio::time::timeout(
        Duration::from_secs(2),
        terminal.wait_for("Failed to start turn"),
    )
    .await??;
    terminal.wait_for("attempt 1/2").await?;
    terminal.send(b"draft remains editable")?;
    tokio::time::timeout(
        Duration::from_secs(2),
        terminal.wait_for("draft remains editable"),
    )
    .await??;
    terminal.send(b"\x15")?;
    terminal
        .wait_for("Connection failed after 2 attempts.")
        .await?;
    if context.model.requests()?.len() != requests {
        return Err("exhausted recovery submitted a message while offline".into());
    }
    // Reconnection alone must not submit the rejected message or continue the
    // old turn. Only the user's next submission may run the new command.
    context.model.script([function(
        "exec_command",
        json!({"cmd":"printf new >> tui-outage-after", "workdir":context.environment.workspace}),
        None,
    )])?;
    drop(offline);
    terminal.type_command("Reconnect this session.").await?;
    terminal
        .wait_for("Execution connection restored. Send your message again.")
        .await?;
    if context.model.requests()?.len() != requests {
        return Err("CLI recovery automatically submitted an unsent message".into());
    }
    terminal
        .type_command("Run only the new task after reconnecting.")
        .await?;
    context.model.wait_for(requests + 2).await?;
    if context.model.requests()?.len() != requests + 2
        || context.environment.read("tui-outage-count").await? != "once"
        || context.environment.read("tui-outage-after").await? != "new"
    {
        return Err("new-message recovery duplicated or lost execution".into());
    }
    context
        .client
        .config()
        .set("test", "reconnect.max_attempts", "10", false, None)
        .await?;
    std::fs::write(
        context.environment.args.output.join("tui-outage.txt"),
        terminal.diagnostics(),
    )?;
    eprintln!(
        "TUI outage: input stayed responsive; recovery waited for the user's resend and ran once"
    );
    Ok(())
}
