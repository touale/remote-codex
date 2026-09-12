use crate::support::{TestResult, run, seed};
use console::measure_text_width;
use portable_pty::CommandBuilder;
use remote_codex_test_support::terminal::Terminal;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn resume_lists_full_ids_and_searches_truncated_titles_without_connecting() -> TestResult {
    let root = tempfile::tempdir()?;
    let server = seed(root.path()).await?;
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(root.path().join("state/state.sqlite3")),
    )
    .await?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let ids = [
        "10000000-0000-0000-0000-000000000001",
        "10000000-0000-0000-0000-000000000002",
    ];
    for (index, id) in ids.iter().enumerate() {
        let record = serde_json::json!({
            "server_id": server, "remote_identity": "fixture", "environment_id": "fixture",
            "codex_home": root.path().join("codex"), "codex_version": "fixture",
            "execution_mode": "sandboxed", "revision": 0,
            "session": {"id": id, "title": format!("{} hidden-token-{}", "长标题测试".repeat(50), if index == 0 { "orchid" } else { "zebra" }),
                "cwd": "/workspace/中文目录/project", "created_at": now - 1000,
                "updated_at": now - 120 - index as u64, "archived": false, "state": "idle"}
        });
        sqlx::query("INSERT INTO local_sessions(id,server,record,updated_at) VALUES(?,?,?,?)")
            .bind(id)
            .bind(&server)
            .bind(record.to_string())
            .bind(now as i64)
            .execute(&pool)
            .await?;
    }
    pool.close().await;
    let output = run(root.path(), &["resume", "--all"])?;
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout)?;
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].starts_with("Session") && lines[0].trim_end().ends_with("ID"));
    assert!(lines.iter().all(|line| measure_text_width(line) <= 160));
    for (line, id) in lines[1..].iter().zip(ids) {
        assert!(line.ends_with(id));
        assert!(line.contains("2m"));
        assert!(!line.contains("hidden-token"));
    }
    let json = run(root.path(), &["resume", "--all", "--json"])?;
    let value: serde_json::Value = serde_json::from_slice(&json.stdout)?;
    assert_eq!(value["schema_version"], 4);
    assert_eq!(value["data"]["sessions"][1]["session"]["id"], ids[1]);

    for cancel in [
        b"\x1b".as_slice(),
        b"\x03".as_slice(),
        b"\x1b[99;5u".as_slice(),
        b"\x1b[99;5:1u".as_slice(),
    ] {
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_remote-codex"));
        command.arg("--data-dir");
        command.arg(root.path().join("state"));
        command.args(["resume", "--all"]);
        command.env("HOME", root.path());
        command.env("CODEX_HOME", root.path().join("codex"));
        command.env("PATH", root.path().join("bin"));
        command.env("TERM", "xterm-256color");
        let mut terminal = Terminal::spawn(command).map_err(|error| error.to_string())?;
        terminal
            .wait_for("Resume session")
            .await
            .map_err(|error| error.to_string())?;
        terminal
            .send(b"hidden-token-zebra")
            .map_err(|error| error.to_string())?;
        terminal
            .wait_for("(1/1)")
            .await
            .map_err(|error| error.to_string())?;
        terminal.send(cancel).map_err(|error| error.to_string())?;
        assert!(
            terminal
                .wait_exit()
                .await
                .map_err(|error| error.to_string())?
                .is_some(),
            "cancel {cancel:?}: {}",
            terminal.diagnostics()
        );
        let text = terminal.diagnostics();
        assert!(text.contains("\x1b[?1049l") && text.contains("\x1b[?25h"));
        assert!(text.contains("selection cancelled"));
    }
    assert!(!root.path().join("ssh.log").exists());
    Ok(())
}
