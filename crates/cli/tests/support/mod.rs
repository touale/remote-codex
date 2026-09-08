use remote_codex_client::{connection::SshEndpoint, store::LocalStore};
use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

pub(crate) fn run(root: &Path, args: &[&str]) -> std::io::Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_remote-codex"));
    command
        .arg("--data-dir")
        .arg(root.join("state"))
        .env_remove("REMOTE_CODEX_CONTEXT_ID")
        .env_remove("REMOTE_CODEX_DATA_DIR");
    if root.join("bin").exists() {
        command
            .env("PATH", root.join("bin"))
            .env("RC_TEST_SSH_LOG", root.join("ssh.log"));
    }
    command.args(args).output()
}

pub(crate) async fn seed(root: &Path) -> TestResult<String> {
    let store = LocalStore::open(&root.join("state")).await?;
    let record = store
        .save_connection(&SshEndpoint::parse("root@dev.example", None)?, Some("dev"))
        .await?;
    store
        .save_connection(
            &SshEndpoint::parse("root@prod.example", None)?,
            Some("prod"),
        )
        .await?;
    store.close().await;
    std::fs::create_dir(root.join("bin"))?;
    let ssh = root.join("bin/ssh");
    std::fs::write(
        &ssh,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$RC_TEST_SSH_LOG\"\nexit 255\n",
    )?;
    std::fs::set_permissions(ssh, std::fs::Permissions::from_mode(0o700))?;
    Ok(record.id)
}
