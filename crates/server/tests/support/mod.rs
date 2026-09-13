pub(crate) use remote_codex_test_support::execution::Peer;
pub(crate) type TestResult<T = ()> = remote_codex_test_support::ProbeResult<T>;

pub(crate) fn runtime(
    root: &std::path::Path,
    program: &std::path::Path,
) -> TestResult<remote_codex_protocol::ExecutionRuntime> {
    let package = root.join("runtimes/codex/1.0.0-fixture-linux-x86_64");
    let bin = package.join("bin");
    std::fs::create_dir_all(&bin)?;
    if !bin.join("codex").exists() {
        std::fs::copy(program, bin.join("codex"))?;
    }
    let digest = "a".repeat(64);
    std::fs::write(package.join(".archive-sha256"), &digest)?;
    Ok(remote_codex_protocol::ExecutionRuntime {
        version: "1.0.0-fixture".into(),
        platform: "linux-x86_64".into(),
        archive_sha256: digest,
    })
}
