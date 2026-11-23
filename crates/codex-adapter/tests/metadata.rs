use remote_codex_adapter::{
    engine::Engine,
    program::Launch,
    thread::{Codex, OpenSource, OpenThread, Prepared},
};
use remote_codex_core::session::SessionBinding;
use serde_json::json;
use std::collections::BTreeMap;

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; temporary Codex home, no model turn"]
async fn native_daybreak_preferences_preserve_workspace_binding()
-> Result<(), Box<dyn std::error::Error>> {
    let program = Launch::new(std::env::var("REMOTE_CODEX_TEST_BINARY")?);
    let home = tempfile::tempdir()?;
    let directory = home.path().canonicalize()?;
    let cwd = directory.to_str().ok_or("temporary path is not UTF-8")?;
    std::fs::write(
        home.path().join("config.toml"),
        "sandbox_mode=\"read-only\"\n",
    )?;
    let codex = Codex::start(&program, home.path()).await?;
    let result = async {
        let opened = codex
            .open(OpenThread {
                environment: "local",
                directory: cwd,
                execution_mode: "sandboxed",
                source: OpenSource::New,
                mcp: BTreeMap::new(),
                instructions: String::new(),
            })
            .await?;
        let id = opened.session.id.clone();
        let binding = SessionBinding {
            server_id: "fixture".into(),
            remote_identity: "fixture".into(),
            environment_id: "local".into(),
            codex_home: cwd.into(),
            codex_version: "fixture".into(),
            execution_mode: "sandboxed".into(),
            revision: 1,
            session: opened.session.clone(),
        };
        let thread = codex.bind(opened, binding, false)?;
        let mappings = BTreeMap::new();
        for enabled in [true, false] {
            let Prepared::Operation(request) = thread.prepare(
                "thread/metadata/update",
                json!({"threadId":id, "daybreakEnabled":enabled,
                    "gitInfo":null, "projectId":null}),
                false,
                &mappings,
            )?
            else {
                return Err("metadata update was not forwarded".into());
            };
            let saved = thread.execute(request).await?;
            assert_eq!(saved["thread"]["id"], id);
            assert_eq!(saved["thread"]["daybreakEnabled"], enabled);
            assert_eq!(saved["thread"]["cwd"], cwd);
            let Prepared::Operation(request) = thread.prepare(
                "config/batchWrite",
                json!({"edits":[{"keyPath":"daybreak", "value":enabled,
                    "mergeStrategy":"replace"}], "filePath":null,
                    "expectedVersion":null, "reloadUserConfig":true}),
                false,
                &mappings,
            )?
            else {
                return Err("default preference was not forwarded".into());
            };
            thread.execute(request).await?;
            let Prepared::Operation(request) = thread.prepare(
                "config/read",
                json!({"includeLayers":false, "cwd":cwd}),
                false,
                &mappings,
            )?
            else {
                return Err("config read was not forwarded".into());
            };
            let config = thread.execute(request).await?;
            assert_eq!(config["config"]["daybreak"], enabled);
        }
        for (params, code) in [
            (
                json!({"threadId":"other-thread", "daybreakEnabled":true}),
                "SESSION_MISMATCH",
            ),
            (
                json!({"threadId":id, "daybreakEnabled":true, "cwd":"/other"}),
                "INVALID_NATIVE_SETTINGS",
            ),
        ] {
            let error = thread
                .prepare("thread/metadata/update", params, false, &mappings)
                .err()
                .ok_or("metadata update bypassed workspace binding")?;
            assert_eq!(error.code, code);
        }
        let Prepared::Operation(request) = thread.prepare(
            "thread/read",
            json!({"threadId":id, "includeTurns":false}),
            false,
            &mappings,
        )?
        else {
            return Err("thread read was not forwarded".into());
        };
        let saved = thread.execute(request).await?;
        assert_eq!(saved["thread"]["daybreakEnabled"], false);
        Ok::<_, Box<dyn std::error::Error>>(())
    }
    .await;
    codex.shutdown().await;
    result?;
    let (reopened, _) = Engine::local(&program, home.path()).await?;
    let config = reopened
        .call("config/read", json!({"includeLayers":false}))
        .await;
    reopened.shutdown().await;
    let config = config?;
    assert_eq!(config["config"]["daybreak"], false);
    assert_eq!(config["config"]["sandbox_mode"], "read-only");
    Ok(())
}
