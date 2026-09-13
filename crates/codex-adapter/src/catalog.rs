//! Compatibility of our execution service, independent of upstream Codex releases.
const REQUIRED_SERVICE_CAPABILITIES: &[&str] = &[
    "command-approval-v1",
    "session-permissions-v1",
    "execution-recovery-v1",
];

pub fn compatible_service(protocol: u32, capabilities: &[String]) -> bool {
    protocol == remote_codex_protocol::VERSION
        && REQUIRED_SERVICE_CAPABILITIES
            .iter()
            .all(|required| capabilities.iter().any(|c| c == required))
}
