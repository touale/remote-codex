use crate::support::{TestResult, run, seed};

#[tokio::test]
async fn deferred_cleanup_warns_without_failing_removal_or_polluting_json() -> TestResult {
    use std::os::unix::fs::OpenOptionsExt;
    let root = tempfile::tempdir()?;
    seed(root.path()).await?;
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(root.path().join("state/state.sqlite3")),
    )
    .await?;
    sqlx::query(
        "INSERT INTO credentials(id,state,managed) VALUES('test-owned-retired','retired',1)",
    )
    .execute(&pool)
    .await?;
    pool.close().await;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.path().join("state/credentials.lock"))?;
    let _writer = nix::fcntl::Flock::lock(lock, nix::fcntl::FlockArg::LockSharedNonblock)
        .map_err(|(_, error)| error)?;
    let result = run(root.path(), &["server", "remove", "dev", "--yes", "--json"])?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let response: serde_json::Value = serde_json::from_slice(&result.stdout)?;
    assert_eq!(response["schema_version"], 4);
    assert_eq!(response["data"]["removed"], "dev");
    let warning: serde_json::Value = serde_json::from_slice(&result.stderr)?;
    assert_eq!(warning["code"], "CREDENTIAL_CLEANUP_PENDING");
    assert_eq!(warning["level"], "warning");
    assert!(
        warning["message"]
            .as_str()
            .is_some_and(|message| message.contains("retry"))
    );
    let other = run(root.path(), &["config", "get", "background", "-n", "prod"])?;
    assert!(other.status.success());
    assert_eq!(other.stdout, b"true\n");
    Ok(())
}

#[tokio::test]
async fn configuration_lists_the_supported_public_settings() -> TestResult {
    let root = tempfile::tempdir()?;
    seed(root.path()).await?;
    let result = run(root.path(), &["config", "list", "-n", "dev", "--json"])?;
    let report: serde_json::Value = serde_json::from_slice(&result.stdout)?;
    let keys: Vec<_> = report["data"]["items"]
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
            "reconnect.max_attempts",
            "proxy.mode",
            "execution.mode",
            "ssh.host",
            "ssh.user",
            "ssh.port"
        ]
    );
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
    assert_eq!(report["schema_version"], 4);
    assert_eq!(report["data"]["server_id"], id);
    assert_eq!(report["data"]["saved_revision"], 1);
    assert!(report["data"]["applied_revision"].is_null());
    let background = report["data"]["items"]
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
        serde_json::from_slice::<serde_json::Value>(&stale.stdout)?["error"]["code"],
        "REVISION_CONFLICT"
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
        serde_json::from_slice::<serde_json::Value>(&overrides.stdout)?["data"]["items"],
        serde_json::json!([])
    );
    let other = run(root.path(), &["config", "list", "-n", "prod", "--json"])?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&other.stdout)?["data"]["saved_revision"],
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
    }
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
