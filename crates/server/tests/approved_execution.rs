mod support;
use remote_codex_protocol::{Call, CommandApproval, Frame, RemoteConfig, Request, VERSION, codec};
use remote_codex_server::{paths, service::Service};
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
use support::{Peer, TestResult};

#[tokio::test]
#[ignore = "requires real Codex exec-server; verifies one-time approval and replay protection"]
async fn sandboxed_execution_requires_exact_approval_and_replays_once() -> TestResult {
    let root = tempfile::Builder::new()
        .prefix("rc-approval-")
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
    let result = async {
        service.dispatch(&call(Request::Configure(RemoteConfig {revision:1,values:BTreeMap::from([("execution.mode".into(),"sandboxed".into())])}))).await?;
        let channel=uuid::Uuid::new_v4().to_string();
        service.dispatch(&call(Request::OpenExecution { runtime: support::runtime(root.path(), &codex)?,channel:channel.clone(), revision:1,mcp:vec![]})).await?;
        let mut peer=Peer::attach(&socket,&service.store.identity,&channel,0).await?;
        peer.call("initialize",json!({"clientName":"approved-command-test"})).await?;
        peer.send(&uuid::Uuid::new_v4().to_string(),json!({"method":"initialized","params":{}})).await?;
        let argv=vec!["/bin/sh".into(),"-c".into(),"printf once >> count".into()];
        let cwd=url::Url::from_file_path(&workspace).map_err(|_|"invalid URI")?.to_string();
        let params=json!({"processId":"approved","argv":argv,"cwd":cwd,"env":{"CODEX_THREAD_ID":"thread"},"tty":false});
        assert!(peer.call("process/start",params.clone()).await.is_err());
        assert!(!workspace.join("count").exists());
        let grant=CommandApproval {id:uuid::Uuid::new_v4().to_string(),channel:channel.clone(),thread:"thread".into(),turn:"turn".into(),item:"item".into(),argv,cwd};
        let message=json!({"id":1,"method":"process/start","params":params});
        let frame=Frame::Execute {operation:grant.id.clone(),message:message.clone(),approval:Some(Box::new(grant.clone())),permissions:None};
        codec::write(&mut peer.writer,&frame).await?;
        peer.response().await?;
        let cursor=peer.detach().await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let mut peer=Peer::attach(&socket,&service.store.identity,&channel,cursor).await?;
        codec::write(&mut peer.writer,&frame).await?;
        peer.response().await?;
        assert_eq!(tokio::fs::read_to_string(workspace.join("count")).await?,"once");
        codec::write(&mut peer.writer,&Frame::Execute {operation:uuid::Uuid::new_v4().to_string(),message,approval:Some(Box::new(grant)),permissions:None}).await?;
        assert!(peer.response().await.is_err());
        assert_eq!(service.store.jobs("test",None).await?.len(),1);
        peer.detach().await?;
        Ok::<_,Box<dyn std::error::Error+Send+Sync>>(())
    }.await;
    service.shutdown().await;
    task.abort();
    result
}
