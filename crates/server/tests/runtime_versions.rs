use remote_codex_protocol::{Call, ExecutionRuntime, RemoteConfig, Request, VERSION};
use remote_codex_server::service::Service;
use remote_codex_test_support::execution::Peer;
use serde_json::json;
use std::{collections::BTreeMap, os::unix::fs::PermissionsExt, path::Path};
type TestResult = remote_codex_test_support::ProbeResult;

fn package(
    root: &Path,
    version: &str,
) -> Result<ExecutionRuntime, Box<dyn std::error::Error + Send + Sync>> {
    let dir = root
        .join("runtimes/codex")
        .join(format!("{version}-linux-x86_64"));
    std::fs::create_dir_all(dir.join("bin"))?;
    let program = dir.join("bin/codex");
    std::fs::write(
        &program,
        format!(
            r#"#!/bin/sh
while IFS= read -r line; do
    id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
    if test -n "$id"; then printf '{{"id":"%s","result":{{"version":"{version}"}}}}\n' "$id"; fi
done
"#
        ),
    )?;
    std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o700))?;
    let digest = "a".repeat(64);
    std::fs::write(dir.join(".archive-sha256"), &digest)?;
    Ok(ExecutionRuntime {
        version: version.into(),
        platform: "linux-x86_64".into(),
        archive_sha256: digest,
    })
}

#[tokio::test]
async fn channels_keep_their_selected_runtime_across_other_session_starts() -> TestResult {
    let root = tempfile::tempdir()?;
    let first = package(root.path(), "1.2.3")?;
    let second = package(root.path(), "1.2.4")?;
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
    let result = async {
        service
            .dispatch(&call(Request::Configure(RemoteConfig {
                values: BTreeMap::new(),
                revision: 1,
            })))
            .await?;
        let one = uuid::Uuid::new_v4().to_string();
        let two = uuid::Uuid::new_v4().to_string();
        service
            .dispatch(&call(Request::OpenExecution {
                channel: one.clone(),
                revision: 1,
                runtime: first,
                mcp: vec![],
            }))
            .await?;
        let mut old = Peer::attach(&socket, &service.store.identity, &one, 0).await?;
        assert_eq!(old.call("initialize", json!({})).await?["version"], "1.2.3");
        service
            .dispatch(&call(Request::OpenExecution {
                channel: two.clone(),
                revision: 1,
                runtime: second.clone(),
                mcp: vec![],
            }))
            .await?;
        let mut new = Peer::attach(&socket, &service.store.identity, &two, 0).await?;
        assert_eq!(new.call("initialize", json!({})).await?["version"], "1.2.4");
        assert_eq!(
            old.call("environment/info", json!({})).await?["version"],
            "1.2.3"
        );
        let changed = service
            .dispatch(&call(Request::OpenExecution {
                channel: one,
                revision: 1,
                runtime: second,
                mcp: vec![],
            }))
            .await;
        assert_eq!(
            changed.err().ok_or("expected channel conflict")?.code,
            "EXECUTION_CONFLICT"
        );
        let invalid = ExecutionRuntime {
            version: "../../outside".into(),
            platform: "linux-x86_64".into(),
            archive_sha256: "a".repeat(64),
        };
        assert!(
            service
                .dispatch(&call(Request::OpenExecution {
                    channel: uuid::Uuid::new_v4().to_string(),
                    revision: 1,
                    runtime: invalid,
                    mcp: vec![]
                }))
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
