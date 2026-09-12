mod support;
use remote_codex_protocol::{Call, RemoteConfig, Request, VERSION};
use remote_codex_server::{paths, service::Service};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use support::{Peer, TestResult};

async fn call(service: &Arc<Service>, request: Request) -> TestResult<Value> {
    Ok(service
        .dispatch(&Call {
            protocol: VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            profile: "test".into(),
            expected_identity: Some(service.store.identity.clone()),
            request,
        })
        .await?)
}

#[tokio::test]
#[ignore = "requires the pinned real Codex exec-server; uses only isolated temporary files"]
async fn native_execution_survives_detach_deduplicates_and_cancels() -> TestResult {
    let root = tempfile::Builder::new()
        .prefix("rc-native-exec-")
        .tempdir_in("/tmp")?;
    let workspace = root.path().join("workspace");
    paths::private(&workspace)?;
    let codex = std::env::split_paths(&std::env::var_os("PATH").ok_or("PATH missing")?)
        .map(|p| p.join("codex"))
        .find(|p| p.is_file())
        .ok_or("Codex required")?
        .canonicalize()?;
    let service = Service::open(&root.path().join("state")).await?;
    let socket = root.path().join("peer.sock");
    let listener = tokio::net::UnixListener::bind(&socket)?;
    let supervisor = service.clone();
    let task = tokio::spawn(async move { supervisor.serve(listener).await });
    let test=async {
        call(&service,Request::Configure(RemoteConfig{codex:codex.to_string_lossy().into_owned(),revision:1,values:BTreeMap::from([("execution.mode".into(),"unrestricted".into()),("background".into(),"true".into())])})).await?;
        let channel=uuid::Uuid::new_v4().to_string();
        call(&service,Request::OpenExecution{channel:channel.clone(),revision:1,mcp:vec![]}).await?;
        let mut peer=Peer::attach(&socket,&service.store.identity,&channel,0).await?;
        peer.call("initialize",json!({"clientName":"remote-codex-test"})).await?;
        peer.send(&uuid::Uuid::new_v4().to_string(),json!({"method":"initialized","params":{}})).await?;
        let operation=uuid::Uuid::new_v4().to_string();
        let message=json!({"id":1,"method":"process/start","params":{"processId":"once","argv":["/bin/sh","-c","printf once >> count; sleep 2; printf finished > done; echo retained"],"cwd":url::Url::from_directory_path(&workspace).map_err(|_|"invalid URI")?.to_string(),"env":{"CODEX_THREAD_ID":"local-test-thread"},"tty":false}});
        peer.send(&operation,message.clone()).await?;peer.response().await?;
        let cursor=peer.detach().await?;
        call(&service,Request::PrepareUpdate).await?;
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(tokio::fs::read_to_string(workspace.join("done")).await?,"finished");
        let mut peer=Peer::attach(&socket,&service.store.identity,&channel,cursor).await?;
        peer.send(&operation,message).await?;peer.response().await?;
        assert_eq!(tokio::fs::read_to_string(workspace.join("count")).await?,"once");
        let jobs=call(&service,Request::Jobs{thread:Some("local-test-thread".into())}).await?;
        assert_eq!(jobs[0]["state"],"completed");
        let output=call(&service,Request::JobOutput{id:jobs[0]["id"].as_str().ok_or("job ID missing")?.into(),after:0}).await?;
        assert!(!output["chunks"].as_array().ok_or("chunks missing")?.is_empty());
        peer.call("process/start",json!({"processId":"cancel","argv":["/bin/sh","-c","sleep 5; echo incorrect > cancelled"],"cwd":url::Url::from_directory_path(&workspace).map_err(|_|"invalid URI")?.to_string(),"env":{},"tty":false})).await?;
        peer.call("process/terminate",json!({"processId":"cancel"})).await?;
        peer.detach().await?;
        tokio::time::sleep(Duration::from_secs(6)).await;
        assert!(!workspace.join("cancelled").exists());
        Ok::<_,Box<dyn std::error::Error+Send+Sync>>(())
    }.await;
    service.shutdown().await;
    task.abort();
    test
}
