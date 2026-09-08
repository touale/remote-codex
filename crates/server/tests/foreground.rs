mod support;
use remote_codex_protocol::{Call, RemoteConfig, Request, VERSION};
use remote_codex_server::{paths, service::Service};
use serde_json::json;
use std::collections::BTreeMap;
use support::{Peer, TestResult};

#[tokio::test]
#[ignore = "requires real Codex exec-server; verifies safe default and foreground cleanup"]
async fn sandbox_is_required_and_foreground_detach_terminates_commands() -> TestResult {
    let root = tempfile::Builder::new()
        .prefix("rc-foreground-")
        .tempdir_in("/tmp")?;
    let workspace = root.path().join("workspace");
    paths::private(&workspace)?;
    let codex = std::env::split_paths(&std::env::var_os("PATH").ok_or("PATH missing")?)
        .map(|p| p.join("codex"))
        .find(|p| p.is_file())
        .ok_or("Codex required")?
        .canonicalize()?;
    let service = Service::open(&root.path().join("state")).await?;
    let socket = root.path().join("frontend.sock");
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
    let command = json!({"processId":"reused-native-id","argv":["/bin/sh","-c","sleep 4; echo incorrect > finished"],"cwd":url::Url::from_directory_path(&workspace).map_err(|_|"invalid URI")?.to_string(),"env":{},"tty":false});
    let result = async {
        for (revision, mode) in [(1, "sandboxed"), (2, "unrestricted")] {
            service
                .dispatch(&call(Request::Configure(RemoteConfig {
                    codex: codex.to_string_lossy().into_owned(),
                    revision,
                    values: BTreeMap::from([
                        ("execution.mode".into(), mode.into()),
                        ("background".into(), "false".into()),
                        ("disconnect_grace_seconds".into(), "0".into()),
                    ]),
                })))
                .await?;
            let channel = uuid::Uuid::new_v4().to_string();
            service
                .dispatch(&call(Request::OpenExecution {
                    channel: channel.clone(),
                    revision,
                    mcp: vec![],
                }))
                .await?;
            let mut peer = Peer::attach(&socket, &service.store.identity, &channel, 0).await?;
            peer.call("initialize", json!({"clientName":"foreground-test"}))
                .await?;
            peer.send(
                &uuid::Uuid::new_v4().to_string(),
                json!({"method":"initialized","params":{}}),
            )
            .await?;
            let response = peer.call("process/start", command.clone()).await;
            if mode == "sandboxed" {
                assert!(response.is_err());
            } else {
                response?;
            }
            peer.detach().await?;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        assert!(!workspace.join("finished").exists());
        let jobs = service.store.jobs("test", None).await?;
        assert_eq!(jobs.len(), 1);
        assert_ne!(jobs[0].state, "running");
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;
    service.shutdown().await;
    task.abort();
    result
}
