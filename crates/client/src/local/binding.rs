use super::*;

pub(super) fn validate(
    binding: &SessionBinding,
    remote: &Remote,
    home: &Path,
    mode: &str,
) -> Result<()> {
    if binding.server_id != remote.server.id || binding.remote_identity != remote.identity.identity
    {
        return Err(ClientError::Argument(
            "session belongs to another execution environment",
        ));
    }
    if Path::new(&binding.codex_home) != home {
        return Err(ClientError::Argument(
            "session belongs to a different local CODEX_HOME",
        ));
    }
    if binding.execution_mode != mode {
        return Err(ClientError::Argument(
            "environment execution mode changed; create a new session to use the new permissions",
        ));
    }
    if binding.codex_version != remote_codex_adapter::catalog::VERSION {
        return Err(ClientError::Unsupported(
            "session Codex version has no validated migration",
        ));
    }
    Ok(())
}
