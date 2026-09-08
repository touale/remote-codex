//! One compatibility entry for all native process and package consumers.
pub const VERSION: &str = "0.153.4";
pub const REMOTE_TARGET: &str = "linux-x86_64";
pub const REMOTE_SHA256: &str = "a822187e1a2420c61c5926721bfbd878701ed95547c9bb0d4de4498a16ba1821";
pub const REMOTE_URL: &str = "https://github.com/openai/codex/releases/download/rust-v0.153.4/codex-package-x86_64-unknown-linux-musl.tar.gz";
pub const REQUIRED_SERVICE_CAPABILITIES: &[&str] =
    &["command-approval-v1", "session-permissions-v1"];

pub fn compatible_service(protocol: u32, capabilities: &[String]) -> bool {
    protocol == remote_codex_protocol::VERSION
        && REQUIRED_SERVICE_CAPABILITIES
            .iter()
            .all(|required| capabilities.iter().any(|c| c == required))
}
