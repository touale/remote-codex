use super::{Context, headless};
use remote_codex_client::{
    application::{Client, ClientOptions, OpenSession, ServiceBundle},
    progress::{PrepareEvent, PrepareStage},
    session::{PermissionPreset, SessionSettings},
};
use remote_codex_test_support::{ProbeResult, model::function};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let session = headless::open(context, None, false).await?;
    session
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    context.model.script([function("exec_command", json!({"cmd":"printf once >> background-count; sleep 12; printf finished > background-done", "workdir":context.environment.workspace,"yield_time_ms":1000}), None)])?;
    headless::turn(&session, false).await?;
    session.close().await;
    // An appended ELF trailer changes the build digest without changing its
    // protocol or behavior, modeling a compatible service rebuild.
    let mut bytes = std::fs::read(&context.environment.args.service)?;
    bytes.extend_from_slice(b"\nremote-codex acceptance build\n");
    let deferred = Arc::new(AtomicBool::new(false));
    let observed = deferred.clone();
    let client = Client::open(ClientOptions {
        data_dir: Some(context.environment.state.clone()),
        ssh_config: context.environment.args.ssh_config.clone(),
        interactive: false,
        notice: None,
        service: ServiceBundle {
            sha256: super::sha256(&bytes),
            bytes: Arc::from(bytes),
        },
        progress: Some(Arc::new(move |event| {
            if matches!(event, PrepareEvent::Stage(PrepareStage::UseRunningService)) {
                observed.store(true, Ordering::Release);
            }
        })),
        ..Default::default()
    })
    .await?;
    let options = OpenSession {
        server: "test".into(),
        path: context.environment.workspace.clone(),
        resume: None,
        takeover: false,
        mcp_source: None,
    };
    let opened = client
        .sessions()
        .prepare(options)
        .await?
        .open(false)
        .await?;
    if !deferred.load(Ordering::Acquire) {
        return Err("busy compatible service did not defer its upgrade".into());
    }
    opened.close().await;
    client.close().await;
    context
        .environment
        .command(&format!(
            "while ! test -f {}; do sleep 1; done",
            super::environment::quote(&format!(
                "{}/background-done",
                context.environment.workspace
            ))?
        ))
        .await?;
    if context.environment.read("background-count").await? != "once" {
        return Err("background command was replayed or lost".into());
    }
    let foreground = headless::open(context, None, false).await?;
    foreground
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    context.model.script([function("exec_command", json!({"cmd":"sleep 5; printf forbidden > foreground-done","workdir":context.environment.workspace,"yield_time_ms":1000}), None)])?;
    headless::turn(&foreground, false).await?;
    context
        .client
        .config()
        .set("test", "disconnect_grace_seconds", "0", false, None)
        .await?;
    context
        .client
        .config()
        .set("test", "background", "false", false, None)
        .await?;
    foreground.close().await;
    tokio::time::sleep(Duration::from_secs(6)).await;
    if !context.environment.absent("foreground-done").await? {
        return Err("updated foreground policy did not stop detached work".into());
    }
    eprintln!(
        "lifecycle: compatible busy update, background preservation and live foreground configuration passed"
    );
    Ok(())
}
