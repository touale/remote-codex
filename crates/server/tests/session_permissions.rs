mod support;
use remote_codex_protocol::{
    Call, Frame, RemoteConfig, Request, SessionPermissions, VERSION, codec,
};
use remote_codex_server::{paths, service::Service};
use serde_json::json;
use std::collections::BTreeMap;
use support::{Peer, TestResult};

#[tokio::test]
#[ignore = "requires real Codex exec-server; verifies channel-bound Full Access file writes"]
async fn session_permissions_do_not_change_the_server_default_or_another_channel() -> TestResult {
    let root = tempfile::Builder::new()
        .prefix("rc-full-permissions-")
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
    let owner = service.clone();
    let task = tokio::spawn(async move { owner.serve(listener).await });
    let call = |request| Call {
        protocol: VERSION,
        id: uuid::Uuid::new_v4().to_string(),
        profile: "test".into(),
        expected_identity: Some(service.store.identity.clone()),
        request,
    };
    let result=async{
        service.dispatch(&call(Request::Configure(RemoteConfig{codex:codex.to_string_lossy().into_owned(),revision:1,values:BTreeMap::from([("execution.mode".into(),"sandboxed".into())])}))).await?;
        let mut grant=None;
        for index in 0..2 {
            let channel=uuid::Uuid::new_v4().to_string();
            service.dispatch(&call(Request::OpenExecution{channel:channel.clone(),revision:1,mcp:vec![]})).await?;
            let mut peer=Peer::attach(&socket,&service.store.identity,&channel,0).await?;
            peer.call("initialize",json!({"clientName":"session-permission-test"})).await?;
            peer.send(&uuid::Uuid::new_v4().to_string(),json!({"method":"initialized","params":{}})).await?;
            let file=workspace.join(format!("{index}.txt"));
            let params=json!({"dataBase64":"MTIzCg==","path":url::Url::from_file_path(&file).map_err(|_|"invalid URI")?.to_string(),"sandbox":null});
            assert!(peer.call("fs/writeFile",params.clone()).await.is_err());
            if index==0 {grant=Some(Box::new(SessionPermissions{id:uuid::Uuid::new_v4().to_string(),channel,thread:"thread".into()}));}
            let frame=Frame::Execute{operation:uuid::Uuid::new_v4().to_string(),message:json!({"id":1,"method":"fs/writeFile","params":params}),approval:None,permissions:grant.clone()};
            codec::write(&mut peer.writer,&frame).await?;
            if index==0 {
                peer.response().await?;
                assert_eq!(tokio::fs::read_to_string(&file).await?,"123\n");
                // A subsequent unadorned request cannot reuse the previous scope.
                assert!(peer.call("fs/writeFile",params).await.is_err());
            } else {assert!(peer.response().await.is_err());assert!(!file.exists());}
            peer.detach().await?;
        }
        assert_eq!(service.store.config("test").await?.values["execution.mode"],"sandboxed");
        Ok::<_,Box<dyn std::error::Error+Send+Sync>>(())
    }.await;
    service.shutdown().await;
    task.abort();
    result
}
