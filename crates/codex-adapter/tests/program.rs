use remote_codex_adapter::program::inspect;
use std::path::PathBuf;

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; isolated version and protocol inspection only"]
async fn installed_release_is_detected_without_a_version_allowlist()
-> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(std::env::var("REMOTE_CODEX_TEST_BINARY")?);
    let program = inspect(&path).await?;
    let again = inspect(&path).await?;
    assert_eq!(program.version, again.version);
    assert_eq!(program.path, path.canonicalize()?);
    program.verify()?;
    println!(
        "Codex {}: required protocol contracts available",
        program.version
    );
    Ok(())
}
