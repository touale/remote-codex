mod support;
use remote_codex_protocol::{Call, RemoteConfig, Request, VERSION};
use remote_codex_server::service::Service;
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
use support::{Peer, TestResult};

#[tokio::test]
#[ignore = "requires real Codex; verifies maintenance only retires explicitly detached idle executors"]
async fn maintenance_preserves_open_sessions_and_retires_detached_idle_executors() -> TestResult {
    let root = tempfile::Builder::new()
        .prefix("rc-idle-retire-")
        .tempdir_in("/tmp")?;
    let codex = std::env::split_paths(&std::env::var_os("PATH").ok_or("PATH missing")?)
        .map(|p| p.join("codex"))
        .find(|p| p.is_file())
        .ok_or("Codex required")?
        .canonicalize()?;
    let state = root.path().join("state");
    let service = Service::open(&state).await?;
    let socket = root.path().join("peer.sock");
    let listener = tokio::net::UnixListener::bind(&socket)?;
    let owner = service.clone();
    let task = tokio::spawn(async move { owner.serve(listener).await });
    let call = |request| Call {
        protocol: VERSION,
        id: uuid::Uuid::new_v4().to_string(),
        profile: "test".into(),
        expected_identity: Some(service.store.identity.clone()),
        request,
    };
    let result = async {
        service
            .dispatch(&call(Request::Configure(RemoteConfig {
                codex: codex.to_string_lossy().into_owned(),
                revision: 1,
                values: BTreeMap::from([
                    ("execution.mode".into(), "sandboxed".into()),
                    ("background".into(), "true".into()),
                ]),
            })))
            .await?;
        let channel = uuid::Uuid::new_v4().to_string();
        service
            .dispatch(&call(Request::OpenExecution {
                channel: channel.clone(),
                revision: 1,
                mcp: vec![],
            }))
            .await?;
        let mut peer = Peer::attach(&socket, &service.store.identity, &channel, 0).await?;
        peer.call("initialize", json!({"clientName":"idle-retirement-test"}))
            .await?;
        peer.send(
            &uuid::Uuid::new_v4().to_string(),
            json!({"method":"initialized","params":{}}),
        )
        .await?;
        service.dispatch(&call(Request::PrepareUpdate)).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        peer.call("environment/info", json!({})).await?;
        let cursor = peer.cursor;
        drop(peer);
        service.dispatch(&call(Request::PrepareUpdate)).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;
        let mut peer = Peer::attach(&socket, &service.store.identity, &channel, cursor).await?;
        peer.call("environment/info", json!({})).await?;
        peer.detach().await?;
        service
            .dispatch(&call(Request::DetachExecution {
                channel: channel.clone(),
            }))
            .await?;
        service.dispatch(&call(Request::PrepareUpdate)).await?;
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(state.join("execution.sqlite3"))
            .read_only(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let state: String =
                    sqlx::query_scalar("SELECT state FROM execution_channels WHERE id=?")
                        .bind(&channel)
                        .fetch_one(&pool)
                        .await?;
                if state == "lost" {
                    return Ok::<_, sqlx::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await??;
        pool.close().await;
        assert!(
            Peer::attach(&socket, &service.store.identity, &channel, 0)
                .await
                .is_err()
        );
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;
    service.shutdown().await;
    task.abort();
    result
}
