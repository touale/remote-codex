//! Read the last native 0.153.4 usage report without maintaining a second history.
use remote_codex_core::status::TokenUsage;
use serde_json::Value;
use std::{io, path::Path};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, BufReader, SeekFrom};

const CHUNK: usize = 64 * 1024;
const MAX_RECORD: usize = 1024 * 1024;

pub(crate) async fn read(home: &Path, thread: &str, path: Option<&str>) -> Option<TokenUsage> {
    read_checked(home, thread, Path::new(path?))
        .await
        .ok()
        .flatten()
}

async fn read_checked(home: &Path, thread: &str, path: &Path) -> io::Result<Option<TokenUsage>> {
    let home = tokio::fs::canonicalize(home).await?;
    let path = tokio::fs::canonicalize(path).await?;
    if !["sessions", "archived_sessions"]
        .iter()
        .any(|root| path.starts_with(home.join(root)))
        || !tokio::fs::metadata(&path).await?.is_file()
    {
        return Ok(None);
    }
    let mut file = tokio::fs::File::open(path).await?;
    let mut first = Vec::new();
    BufReader::new((&mut file).take((MAX_RECORD + 1) as u64))
        .read_until(b'\n', &mut first)
        .await?;
    if first.len() > MAX_RECORD || first.last() != Some(&b'\n') {
        return Ok(None);
    }
    let metadata: Value = match serde_json::from_slice(&first) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    if metadata["type"] != "session_meta" || metadata["payload"]["id"].as_str() != Some(thread) {
        return Ok(None);
    }
    // Capture the file extent; a partial last record is never mistaken for a report.
    let mut position = file.metadata().await?.len();
    let mut chunk = vec![0; CHUNK];
    let mut record = Vec::new();
    let mut complete = false;
    let mut oversized = false;
    while position > 0 {
        let length = position.min(CHUNK as u64) as usize;
        position -= length as u64;
        file.seek(SeekFrom::Start(position)).await?;
        file.read_exact(&mut chunk[..length]).await?;
        for &byte in chunk[..length].iter().rev() {
            if byte == b'\n' {
                if complete && !oversized {
                    record.reverse();
                    if let Some(usage) = report(&record) {
                        return Ok(Some(usage));
                    }
                }
                record.clear();
                oversized = false;
                complete = true;
            } else if complete && !oversized {
                if record.len() == MAX_RECORD {
                    record.clear();
                    oversized = true;
                } else {
                    record.push(byte);
                }
            }
        }
    }
    Ok(None)
}

fn report(bytes: &[u8]) -> Option<TokenUsage> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    if value["type"] != "event_msg" || value["payload"]["type"] != "token_count" {
        return None;
    }
    let info = &value["payload"]["info"];
    crate::status::tokens(
        &info["last_token_usage"],
        &info["total_token_usage"],
        &info["model_context_window"],
    )
}

#[cfg(test)]
mod tests;
