use super::*;
use serde_json::json;
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn report(last: u64, total: u64) -> String {
    json!({"type":"event_msg","payload":{"type":"token_count","info":{
        "last_token_usage":{"total_tokens":last,"input_tokens":last-2,"cached_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":1},
        "total_token_usage":{"total_tokens":total},"model_context_window":258400
    }}}).to_string() + "\n"
}
async fn fixture(contents: &str) -> Result<(tempfile::TempDir, std::path::PathBuf)> {
    let home = tempfile::tempdir()?;
    let directory = home.path().join("sessions");
    tokio::fs::create_dir(&directory).await?;
    let path = directory.join("rollout.jsonl");
    let metadata = json!({"type":"session_meta","payload":{"id":"bound-thread"}});
    tokio::fs::write(&path, format!("{metadata}\n{contents}")).await?;
    Ok((home, path))
}

#[tokio::test]
async fn restores_last_native_report_across_chunks_without_comparing_token_sizes() -> Result {
    let (home, path) = fixture(
        &(report(1000, 9000) + &" ".repeat(CHUNK + 25) + "\n" + &report(50, 7000) + "{\"type\":"),
    )
    .await?;
    let usage = read(home.path(), "bound-thread", path.to_str())
        .await
        .ok_or("Expected native usage")?;
    assert_eq!(usage.last_tokens, 50);
    assert_eq!(usage.total_tokens, 7000);
    assert_eq!(usage.context_window, Some(258400));
    assert_eq!(usage.cached_input_tokens, 1);
    Ok(())
}

#[tokio::test]
async fn skips_corrupt_null_and_oversized_records_but_never_guesses_usage() -> Result {
    let contents = report(40, 200)
        + &"x".repeat(MAX_RECORD + CHUNK)
        + "\n"
        + "{invalid}\n"
        + "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":null}}\n";
    let (home, path) = fixture(&contents).await?;
    assert_eq!(
        read(home.path(), "bound-thread", path.to_str())
            .await
            .ok_or("Expected native usage")?
            .last_tokens,
        40
    );
    tokio::fs::write(
        &path,
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"bound-thread\"}}\n",
    )
    .await?;
    assert!(
        read(home.path(), "bound-thread", path.to_str())
            .await
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn verifies_home_and_thread_and_supports_archived_native_history() -> Result {
    let (home, path) = fixture(&report(40, 200)).await?;
    assert!(
        read(home.path(), "another-thread", path.to_str())
            .await
            .is_none()
    );
    let other = tempfile::tempdir()?;
    assert!(
        read(other.path(), "bound-thread", path.to_str())
            .await
            .is_none()
    );
    let outside = home.path().join("outside.jsonl");
    tokio::fs::rename(&path, &outside).await?;
    std::os::unix::fs::symlink(&outside, &path)?;
    assert!(
        read(home.path(), "bound-thread", path.to_str())
            .await
            .is_none()
    );
    let archived = home.path().join("archived_sessions");
    tokio::fs::create_dir(&archived).await?;
    let restored = archived.join("rollout.jsonl");
    tokio::fs::rename(&outside, &restored).await?;
    assert!(
        read(home.path(), "bound-thread", restored.to_str())
            .await
            .is_some()
    );
    Ok(())
}
