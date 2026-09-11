use serde::{Serialize, de::DeserializeOwned};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Callers drop the stream if this future is cancelled; partial packets are never replayed.
pub async fn read<T: DeserializeOwned>(
    input: &mut (impl AsyncRead + Unpin),
) -> io::Result<(T, Vec<u8>)> {
    let header = input.read_u32().await? as usize;
    let size = input.read_u32().await? as usize;
    if header > crate::MAX_FRAME || size > super::CHUNK {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Transfer packet exceeds its limit.",
        ));
    }
    let mut json = vec![0; header];
    input.read_exact(&mut json).await?;
    let value = serde_json::from_slice(&json).map_err(io::Error::other)?;
    let mut bytes = vec![0; size];
    input.read_exact(&mut bytes).await?;
    Ok((value, bytes))
}
pub async fn write(
    output: &mut (impl AsyncWrite + Unpin),
    value: &impl Serialize,
    bytes: &[u8],
) -> io::Result<()> {
    let json = serde_json::to_vec(value).map_err(io::Error::other)?;
    if json.len() > crate::MAX_FRAME || bytes.len() > super::CHUNK {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Transfer packet exceeds its limit.",
        ));
    }
    output.write_u32(json.len() as u32).await?;
    output.write_u32(bytes.len() as u32).await?;
    output.write_all(&json).await?;
    output.write_all(bytes).await?;
    output.flush().await
}
