mod support;

use support::{TestResult, run, seed};

#[tokio::test]
async fn retired_update_policy_is_absent_and_cannot_be_read_or_written() -> TestResult {
    let root = tempfile::tempdir()?;
    seed(root.path()).await?;
    let result = run(root.path(), &["config", "list", "-n", "dev", "--json"])?;
    let report: serde_json::Value = serde_json::from_slice(&result.stdout)?;
    let keys: Vec<_> = report["items"]
        .as_array()
        .ok_or("missing items")?
        .iter()
        .filter_map(|item| item["key"].as_str())
        .collect();
    assert_eq!(
        keys,
        [
            "background",
            "disconnect_grace_seconds",
            "proxy.mode",
            "execution.mode",
            "ssh.host",
            "ssh.user",
            "ssh.port"
        ]
    );
    for mut args in [
        vec!["config", "get", "codex.update_policy"],
        vec!["config", "set", "codex.update_policy", "manual"],
        vec!["config", "unset", "codex.update_policy"],
    ] {
        args.extend(["-n", "dev", "--json"]);
        let rejected = run(root.path(), &args)?;
        assert_eq!(rejected.status.code(), Some(2));
    }
    assert!(!root.path().join("ssh.log").exists());
    Ok(())
}

#[tokio::test]
async fn configuration_is_persistent_per_server_and_syncs_only_its_target() -> TestResult {
    let root = tempfile::tempdir()?;
    let id = seed(root.path()).await?;
    let written = run(
        root.path(),
        &[
            "config",
            "set",
            "background",
            "false",
            "-n",
            "dev",
            "--json",
        ],
    )?;
    assert!(
        written.status.success(),
        "{}",
        String::from_utf8_lossy(&written.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&written.stdout)?;
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["server_id"], id);
    assert!(report.get("global_revision").is_none());
    assert!(report.get("scope").is_none());
    assert_eq!(report["saved_revision"], 1);
    assert!(report["applied_revision"].is_null());
    let background = report["items"]
        .as_array()
        .ok_or("missing items")?
        .iter()
        .find(|item| item["key"] == "background")
        .ok_or("missing background")?;
    assert_eq!(background["value"], false);
    assert_eq!(background["source"], "server");
    let log = std::fs::read_to_string(root.path().join("ssh.log"))?;
    assert!(log.lines().any(|line| line == "dev.example"));
    assert!(!log.contains("prod.example"));
    for (server, value) in [
        ("dev", b"false\n".as_slice()),
        ("prod", b"true\n".as_slice()),
    ] {
        let read = run(root.path(), &["config", "get", "background", "-n", server])?;
        assert!(read.status.success());
        assert_eq!(read.stdout, value);
    }
    assert_eq!(
        std::fs::read_to_string(root.path().join("ssh.log"))?,
        log,
        "reads must not connect"
    );
    let stale = run(
        root.path(),
        &[
            "config",
            "set",
            "background",
            "true",
            "-n",
            "dev",
            "--if-revision",
            "0",
            "--json",
        ],
    )?;
    assert_eq!(stale.status.code(), Some(6));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&stale.stdout)?["error"]["exit_code"],
        6
    );
    let removed = run(
        root.path(),
        &["config", "unset", "background", "-n", "dev", "--json"],
    )?;
    assert!(removed.status.success());
    let defaults = run(root.path(), &["config", "get", "background", "-n", "dev"])?;
    assert_eq!(defaults.stdout, b"true\n");
    let overrides = run(
        root.path(),
        &["config", "list", "--overrides", "-n", "dev", "--json"],
    )?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&overrides.stdout)?["items"],
        serde_json::json!([])
    );
    let other = run(root.path(), &["config", "list", "-n", "prod", "--json"])?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&other.stdout)?["saved_revision"],
        0
    );
    Ok(())
}

#[test]
fn every_config_command_requires_a_server_before_opening_state() -> TestResult {
    let root = tempfile::tempdir()?;
    for args in [
        vec!["config", "list"],
        vec!["config", "get", "background"],
        vec!["config", "set", "background", "--stdin"],
        vec!["config", "unset", "background"],
    ] {
        let result = run(root.path(), &args)?;
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&result.stderr).contains("specify a server with -n NAME"));
        assert!(!root.path().join("state").exists());
        let mut global = args.clone();
        global.extend(["--global", "-n", "dev"]);
        let result = run(root.path(), &global)?;
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&result.stderr).contains("unexpected argument '--global'"));
        assert!(!root.path().join("state").exists());
    }
    Ok(())
}

#[tokio::test]
async fn a_single_saved_server_is_not_an_implicit_config_target() -> TestResult {
    let root = tempfile::tempdir()?;
    let store = remote_codex_client::store::LocalStore::open(&root.path().join("state")).await?;
    store
        .save_connection(
            &remote_codex_client::connection::SshEndpoint::parse("dev.example", None)?,
            Some("dev"),
        )
        .await?;
    store.close().await;
    assert_eq!(
        run(root.path(), &["config", "list"])?.status.code(),
        Some(2)
    );
    Ok(())
}

#[tokio::test]
async fn sensitive_configuration_errors_do_not_expose_input() -> TestResult {
    let root = tempfile::tempdir()?;
    seed(root.path()).await?;
    let result = run(
        root.path(),
        &[
            "config",
            "set",
            "env.https_proxy",
            "http://alice:never-log-this@proxy.example",
            "-n",
            "dev",
            "--json",
        ],
    )?;
    assert_eq!(result.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&result.stdout).contains("never-log-this"));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("never-log-this"));
    assert!(!root.path().join("ssh.log").exists());
    Ok(())
}
