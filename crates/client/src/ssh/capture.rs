use super::*;

pub(crate) struct ScriptOutput {
    pub(crate) status: std::process::ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

impl SshTransport {
    /// A machine-readable operation after SSH authentication. Capturing both
    /// streams keeps remote diagnostics out of the CLI's active progress line.
    pub(crate) async fn capture_script(
        &self,
        endpoint: &SshEndpoint,
        script: &str,
    ) -> Result<ScriptOutput> {
        let mut child = self
            .command(endpoint, Some("sh -s"), false)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut input = child.stdin.take().ok_or(ClientError::RemoteResponse)?;
        let stdout = child.stdout.take().ok_or(ClientError::RemoteResponse)?;
        let stderr = child.stderr.take().ok_or(ClientError::RemoteResponse)?;
        let operation = async {
            let send = async {
                input.write_all(script.as_bytes()).await?;
                input.shutdown().await
            };
            let (_, stdout, stderr) = tokio::try_join!(send, bounded(stdout), bounded(stderr))?;
            Ok(ScriptOutput {
                status: child.wait().await?,
                stdout,
                stderr,
            })
        };
        match tokio::time::timeout(Duration::from_secs(30), operation).await {
            Ok(result) => result,
            Err(_) => {
                child.kill().await?;
                Err(ClientError::Timeout)
            }
        }
    }
}

async fn bounded(reader: impl AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(1024 * 1024 + 1).read_to_end(&mut bytes).await?;
    if bytes.len() > 1024 * 1024 {
        return Err(std::io::Error::other("remote diagnostic exceeds limit"));
    }
    Ok(bytes)
}
