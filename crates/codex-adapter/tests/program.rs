use remote_codex_adapter::program::{Launch, inspect};

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; isolated version and protocol inspection only"]
async fn installed_release_is_detected_without_a_version_allowlist()
-> Result<(), Box<dyn std::error::Error>> {
    let launch = Launch::new(std::env::var("REMOTE_CODEX_TEST_BINARY")?);
    let program = inspect(&launch).await?;
    let again = inspect(&launch).await?;
    assert_eq!(program.version, again.version);
    assert_eq!(program.path, launch.path.canonicalize()?);
    program.verify()?;
    println!(
        "Codex {}: required protocol contracts available",
        program.version
    );
    Ok(())
}
