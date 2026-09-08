use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Count bytes only after a successful write. Backpressure from SSH is preserved.
pub(super) async fn copy_with_progress(
    mut source: impl AsyncRead + Unpin,
    mut destination: impl AsyncWrite + Unpin,
    progress: impl Fn(u64),
) -> std::io::Result<u64> {
    let mut buffer = vec![0; 64 * 1024];
    let mut transferred = 0;
    progress(transferred);
    loop {
        let count = source.read(&mut buffer).await?;
        if count == 0 {
            return Ok(transferred);
        }
        let mut remaining = &buffer[..count];
        while !remaining.is_empty() {
            let written = destination.write(remaining).await?;
            if written == 0 {
                return Err(std::io::ErrorKind::WriteZero.into());
            }
            transferred += written as u64;
            progress(transferred);
            remaining = &remaining[written..];
        }
    }
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
