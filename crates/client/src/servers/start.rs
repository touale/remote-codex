use crate::{
    ClientError, Result,
    connection::SshEndpoint,
    ssh::{SshTransport, quote},
};
use remote_codex_protocol::ServiceStart;

/// A busy supervisor remains usable if the subsequent identity handshake
/// satisfies the compatibility catalog. The immutable update waits for idle.
pub(crate) async fn ensure(
    ssh: &SshTransport,
    endpoint: &SshEndpoint,
    program: &str,
    root: &str,
) -> Result<bool> {
    let script = format!(
        "exec {} --state {} ensure --json\n",
        quote(program)?,
        quote(root)?
    );
    let output = ssh.capture_script(endpoint, &script).await?;
    match decode(output.status, &output.stdout, &output.stderr)? {
        ServiceStart::Ready => Ok(false),
        ServiceStart::Waiting(activity) if activity.channels >= 0 && activity.jobs >= 0 => Ok(true),
        ServiceStart::Failed(fault) => Err(fault.into()),
        _ => Err(ClientError::RemoteResponse),
    }
}

fn decode(status: std::process::ExitStatus, stdout: &[u8], stderr: &[u8]) -> Result<ServiceStart> {
    if status.code() == Some(255) {
        return Err(ClientError::Ssh(255));
    }
    let result = serde_json::from_slice::<ServiceStart>(stdout).map_err(|_| {
        let diagnostic: String = String::from_utf8_lossy(stderr)
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(1024)
            .collect();
        ClientError::RemoteFault(
            "SERVICE_START_FAILED".into(),
            if diagnostic.trim().is_empty() {
                "remote service did not return a valid startup result".into()
            } else {
                diagnostic.trim().into()
            },
            false,
        )
    })?;
    if !status.success() && !matches!(result, ServiceStart::Failed(_)) {
        return Err(ClientError::RemoteResponse);
    }
    Ok(result)
}
