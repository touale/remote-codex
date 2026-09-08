use crate::{
    ClientError, Result,
    connection::SshEndpoint,
    progress::{PrepareEvent, PrepareStage},
    ssh::{SshTransport, quote},
};
use remote_codex_protocol::{ServiceActivity, ServiceStart};
use std::time::Duration;

#[cfg(test)]
#[path = "start_tests.rs"]
mod tests;

pub(crate) async fn ensure(
    ssh: &SshTransport,
    endpoint: &SshEndpoint,
    program: &str,
    root: &str,
    server_name: &str,
    progress: &impl Fn(PrepareEvent),
) -> Result<()> {
    let script = format!(
        "exec {} --state {} ensure --json\n",
        quote(program)?,
        quote(root)?
    );
    retry(server_name, Duration::from_secs(90), progress, async || {
        let output = ssh.capture_script(endpoint, &script).await?;
        decode(output.status, &output.stdout, &output.stderr)
    })
    .await
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

async fn retry(
    server_name: &str,
    timeout: Duration,
    progress: &impl Fn(PrepareEvent),
    mut attempt: impl AsyncFnMut() -> Result<ServiceStart>,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut waiting = false;
    loop {
        match attempt().await? {
            ServiceStart::Ready => {
                if waiting {
                    progress(PrepareEvent::Stage(PrepareStage::StartService));
                }
                return Ok(());
            }
            ServiceStart::Waiting(activity) if activity.channels >= 0 && activity.jobs >= 0 => {
                waiting = true;
                progress(PrepareEvent::ServiceWaiting(activity));
                if tokio::time::Instant::now() >= deadline {
                    return Err(busy(activity, server_name));
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            ServiceStart::Failed(fault) => return Err(fault.into()),
            _ => return Err(ClientError::RemoteResponse),
        }
    }
}

fn busy(activity: ServiceActivity, name: &str) -> ClientError {
    let action = if activity.jobs > 0 {
        "Wait for running commands to finish and close other open sessions on this server, then retry this command."
    } else {
        "Close other open sessions on this server, then retry this command shortly."
    };
    ClientError::RemoteFault(
        "SERVICE_UPDATE_BUSY".into(),
        format!(
            "The remote environment {name:?} is still in use, so its update is waiting. {action} Existing work is preserved."
        ),
        false,
    )
}
