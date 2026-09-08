mod support;

use std::process::Command;
use support::{TestResult, run, seed};

#[test]
fn removed_management_commands_never_become_codex_prompts_or_create_state() -> TestResult {
    let root = tempfile::tempdir()?;
    for (arguments, guidance) in [
        (vec!["jobs", "-n", "dev"], "resume --all"),
        (vec!["login"], "codex login"),
        (vec!["login", "status", "-n", "dev"], "codex login"),
        (
            vec!["update", "--check", "-n", "dev"],
            "automatically when connecting",
        ),
    ] {
        let result = run(root.path(), &arguments)?;
        assert_eq!(result.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&result.stderr).contains(guidance));
    }
    let help = run(root.path(), &["--help"])?;
    assert!(help.status.success());
    assert!(!String::from_utf8_lossy(&help.stdout).lines().any(|line| {
        ["jobs ", "login ", "update "]
            .iter()
            .any(|name| line.trim_start().starts_with(name))
    }));
    assert!(!root.path().join("state").exists());
    Ok(())
}

#[test]
fn legacy_shell_init_is_rejected_without_creating_state() -> TestResult {
    let root = tempfile::tempdir()?;
    let first = run(root.path(), &["shell", "init", "zsh"])?;
    let second = run(root.path(), &["shell", "init", "bash"])?;
    assert_eq!(first.status.code(), Some(2));
    assert_eq!(second.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&first.stderr).contains("no longer needed"));
    assert!(!root.path().join("state").exists());
    Ok(())
}

#[tokio::test]
async fn legacy_workspace_configuration_points_to_per_session_path() -> TestResult {
    let root = tempfile::tempdir()?;
    seed(root.path()).await?;
    let result = run(
        root.path(),
        &["config", "set", "workspace", "/project", "-n", "dev"],
    )?;
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--path"));
    Ok(())
}

#[test]
fn legacy_context_environment_cannot_choose_a_configuration_server() -> TestResult {
    let root = tempfile::tempdir()?;
    let result = Command::new(env!("CARGO_BIN_EXE_remote-codex"))
        .arg("--data-dir")
        .arg(root.path().join("state"))
        .env("REMOTE_CODEX_CONTEXT_ID", "old-terminal")
        .args(["config", "set", "background", "false"])
        .output()?;
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("specify a server with -n NAME"));
    Ok(())
}

#[test]
fn server_add_validates_target_before_attempting_ssh() -> TestResult {
    let root = tempfile::tempdir()?;
    let one = run(
        root.path(),
        &[
            "server",
            "add",
            "-n",
            "dev",
            "--addr",
            "root@host;echo injected",
            "-p",
            "43256",
            "--json",
        ],
    )?;
    let two = run(
        root.path(),
        &["--json", "create", "root@host;echo injected", "-p", "43256"],
    )?;
    assert_eq!(one.status.code(), Some(2));
    assert_eq!(two.status.code(), Some(2));
    let listed = run(root.path(), &["server", "list", "--json"])?;
    assert!(listed.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&listed.stdout)?["servers"],
        serde_json::json!([])
    );
    Ok(())
}

#[test]
fn unsupported_codex_commands_never_fall_back_to_local_execution() -> TestResult {
    let root = tempfile::tempdir()?;
    let result = run(root.path(), &["--json", "exec", "hello"])?;
    assert_eq!(result.status.code(), Some(5));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout)?["schema_version"],
        2
    );
    assert!(!root.path().join("state").exists());
    Ok(())
}
