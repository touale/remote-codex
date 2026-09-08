use crate::support::{TestResult, run};

#[test]
fn unknown_commands_fail_before_creating_state() -> TestResult {
    let root = tempfile::tempdir()?;
    for args in [
        vec!["conect"],
        vec!["jobs"],
        vec!["exec", "hello"],
        vec!["shell", "init", "zsh"],
        vec!["server", "status"],
    ] {
        let result = run(root.path(), &args)?;
        assert_eq!(result.status.code(), Some(2));
        assert!(!root.path().join("state").exists());
    }
    Ok(())
}

#[test]
fn server_add_validates_target_before_attempting_ssh() -> TestResult {
    let root = tempfile::tempdir()?;
    let rejected = run(
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
    assert_eq!(rejected.status.code(), Some(2));
    let error: serde_json::Value = serde_json::from_slice(&rejected.stdout)?;
    assert_eq!(error["schema_version"], 4);
    assert_eq!(error["error"]["code"], "INVALID_SSH_TARGET");
    let listed = run(root.path(), &["server", "list", "--json"])?;
    assert!(listed.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&listed.stdout)?["data"]["servers"],
        serde_json::json!([])
    );
    Ok(())
}
