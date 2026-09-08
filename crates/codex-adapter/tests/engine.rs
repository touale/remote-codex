use remote_codex_adapter::engine::Engine;
use serde_json::json;
use std::{os::unix::fs::PermissionsExt, path::Path};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn executable(home: &Path, body: &str) -> std::io::Result<std::path::PathBuf> {
    let path = home.join("codex-fixture");
    std::fs::write(&path, format!("#!/bin/sh\n{body}"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

#[tokio::test]
async fn large_plugin_catalog_preserves_the_engine_connection() -> TestResult {
    let home = tempfile::tempdir()?;
    let program = executable(
        home.path(),
        r#"while IFS= read -r request; do
case "$request" in
  *'"method":"initialize"'*) printf '{"id":1,"result":{}}\n' ;;
  *'"method":"plugin/list"'*)
    printf '{"id":2,"result":{"marketplaces":[{"description":"'
    head -c 8600000 /dev/zero | tr '\000' 'x'
    printf '"}]}}\n' ;;
  *'"method":"model/list"'*) printf '{"id":3,"result":{"data":[]}}\n' ;;
esac
done
"#,
    )?;
    let (engine, _) = Engine::local(&program, home.path()).await?;
    let result = engine.call("plugin/list", json!({})).await;
    let next = engine.call("model/list", json!({})).await;
    engine.shutdown().await;
    assert_eq!(
        result?["marketplaces"][0]["description"]
            .as_str()
            .map(str::len),
        Some(8_600_000)
    );
    assert_eq!(next?["data"], json!([]));
    Ok(())
}

#[tokio::test]
async fn protocol_failure_preserves_the_reason_for_requests_and_frontends() -> TestResult {
    let home = tempfile::tempdir()?;
    let program = executable(
        home.path(),
        r#"while IFS= read -r request; do
case "$request" in
  *'"method":"initialize"'*) printf '{"id":1,"result":{}}\n' ;;
  *'"method":"plugin/list"'*) printf 'invalid-json\n' ;;
esac
done
"#,
    )?;
    let (engine, _) = Engine::local(&program, home.path()).await?;
    let mut events = engine.subscribe();
    let result = engine.call("plugin/list", json!({})).await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), events.recv()).await??;
    engine.shutdown().await;
    let error = result.err().ok_or("expected protocol failure")?;
    assert_eq!(error.code, "CODEX_PROTOCOL_ERROR");
    assert!(error.message.contains("invalid protocol frame"));
    assert!(error.outcome_unknown);
    assert_eq!(event["method"], "remoteCodex/engineClosed");
    assert_eq!(event["params"]["fault"]["code"], "CODEX_PROTOCOL_ERROR");
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicit native Codex executable and home; reads configured plugin metadata without a model turn"]
async fn configured_native_catalog_keeps_serving_requests() -> TestResult {
    let program = std::path::PathBuf::from(std::env::var("REMOTE_CODEX_TEST_BINARY")?);
    let home = std::path::PathBuf::from(std::env::var("REMOTE_CODEX_TEST_NATIVE_HOME")?);
    let (engine, _) = Engine::local(&program, &home).await?;
    let catalog = engine
        .call("plugin/list", json!({"forceRefetch":false}))
        .await;
    let config = engine
        .call("config/read", json!({"includeLayers":false}))
        .await;
    engine.shutdown().await;
    let catalog = catalog?;
    assert!(catalog["marketplaces"].is_array());
    assert!(config?["config"].is_object());
    eprintln!(
        "Native plugin catalog: {} bytes; subsequent config/read succeeded",
        serde_json::to_vec(&catalog)?.len()
    );
    Ok(())
}
