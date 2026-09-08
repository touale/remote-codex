use remote_codex_client::application::{Client, ClientOptions};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
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
        .env_remove("REMOTE_CODEX_DATA_DIR");
    if root.join("bin").exists() {
        command
            .env("PATH", root.join("bin"))
            .env("RC_TEST_SSH_LOG", root.join("ssh.log"));
    }
    command.args(args).output()
}

/// Offline registration fixture. Production initialization is covered by the SSH gate.
pub(crate) async fn seed(root: &Path) -> TestResult<String> {
    let state = root.join("state");
    Client::open(ClientOptions {
        data_dir: Some(state.clone()),
        ..Default::default()
    })
    .await?
    .close()
    .await;
    let mut db = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(state.join("state.sqlite3")),
    )
    .await?;
    sqlx::raw_sql("INSERT INTO server_revisions(id,revision) VALUES('dev-id',0),('prod-id',0); INSERT INTO connections(id,name,endpoint) VALUES('dev-id','dev','{\"host\":\"dev.example\",\"user\":\"root\",\"port\":null}'),('prod-id','prod','{\"host\":\"prod.example\",\"user\":\"root\",\"port\":null}');").execute(&mut db).await?;
    db.close().await?;
    std::fs::create_dir(root.join("bin"))?;
    let ssh = root.join("bin/ssh");
    std::fs::write(
        &ssh,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$RC_TEST_SSH_LOG\"\nexit 255\n",
    )?;
    std::fs::set_permissions(ssh, std::fs::Permissions::from_mode(0o700))?;
    Ok("dev-id".into())
}
