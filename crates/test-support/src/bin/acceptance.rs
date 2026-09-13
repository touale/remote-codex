//! Release gate: missing native binaries or SSH configuration is a failure.
#[path = "../acceptance/mod.rs"]
mod acceptance;

#[tokio::main]
async fn main() -> remote_codex_test_support::ProbeResult<()> {
    acceptance::run().await
}
