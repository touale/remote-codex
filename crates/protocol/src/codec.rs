use crate::MAX_FRAME;
use serde::{Serialize, de::DeserializeOwned};
use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

pub struct Reader<R> {
    input: R,
    bytes: Vec<u8>,
    limit: usize,
}

impl<R: AsyncBufRead + Unpin> Reader<R> {
    pub fn new(input: R) -> Self {
        Self::with_limit(input, MAX_FRAME)
    }

    /// Set a bound for another JSONL transport without changing the wire protocol.
    pub fn with_limit(input: R, limit: usize) -> Self {
        Self {
            input,
            bytes: Vec::new(),
            limit,
        }
    }

    /// Partial frames survive cancellation of this future in select!.
    pub async fn next<T: DeserializeOwned>(&mut self) -> io::Result<Option<T>> {
        loop {
            let chunk = self.input.fill_buf().await?;
            if chunk.is_empty() {
                return if self.bytes.is_empty() {
                    Ok(None)
                } else {
                    Err(io::ErrorKind::UnexpectedEof.into())
                };
            }
            let end = chunk.iter().position(|b| *b == b'\n');
            let count = end.map_or(chunk.len(), |i| i + 1);
            if self.bytes.len() + count > self.limit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("protocol frame exceeds {} byte limit", self.limit),
                ));
            }
            self.bytes.extend_from_slice(&chunk[..count]);
            self.input.consume(count);
            if end.is_some() {
                let result = serde_json::from_slice(&self.bytes).map(Some).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid protocol frame")
                });
                self.bytes.clear();
                return result;
            }
        }
    }
}

pub async fn write(
    output: &mut (impl AsyncWrite + Unpin),
    value: &impl Serialize,
) -> io::Result<()> {
    write_with_limit(output, value, MAX_FRAME).await
}

pub async fn write_with_limit(
    output: &mut (impl AsyncWrite + Unpin),
    value: &impl Serialize,
    limit: usize,
) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("protocol frame exceeds {limit} byte limit"),
        ));
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        output.write_all(&bytes).await?;
        output.flush().await
    })
    .await
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "protocol peer stopped accepting data",
        )
    })?
}
