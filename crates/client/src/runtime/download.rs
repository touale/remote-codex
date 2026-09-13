use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    ClientError, Result,
    progress::{PrepareEvent, PrepareStage, TransferKind, TransferProgress},
    store::private_directory,
};
pub(super) async fn verified_package(
    cache: &Path,
    url: &str,
    digest: &str,
    progress: impl Fn(PrepareEvent),
) -> Result<PathBuf> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .user_agent(concat!("remote-codex/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    package_with(cache, &client, url, digest, progress).await
}

async fn package_with(
    cache: &Path,
    client: &reqwest::Client,
    url: &str,
    expected_sha256: &str,
    progress: impl Fn(PrepareEvent),
) -> Result<PathBuf> {
    private_directory(cache)?;
    let path = cache.join(format!("{expected_sha256}.tar.gz"));
    progress(PrepareEvent::Stage(PrepareStage::InspectCache));
    if path.try_exists()? {
        if checksum(&path).await? == expected_sha256 {
            progress(PrepareEvent::Stage(PrepareStage::UseCachedPackage));
            return Ok(path);
        }
        return Err(ClientError::Integrity);
    }
    progress(PrepareEvent::Stage(PrepareStage::Download));
    let response = client.get(url).send().await?.error_for_status()?;
    let max_size = 512 * 1024 * 1024;
    let total = response.content_length();
    if total.is_some_and(|size| size > max_size) {
        return Err(ClientError::Integrity);
    }
    let report = |transferred_bytes| {
        progress(PrepareEvent::Transfer(TransferProgress {
            kind: TransferKind::Download,
            transferred_bytes,
            total_bytes: total,
        }));
    };
    report(0);
    let temporary = tempfile::NamedTempFile::new_in(cache)?;
    let mut file = tokio::fs::File::from_std(temporary.reopen()?);
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut size = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        size += chunk.len() as u64;
        if size > max_size {
            return Err(ClientError::Integrity);
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        report(size);
    }
    progress(PrepareEvent::Stage(PrepareStage::VerifyDownload));
    file.sync_all().await?;
    drop(file);
    if format!("{:x}", hasher.finalize()) != expected_sha256 {
        return Err(ClientError::Integrity);
    }
    // Concurrent verified downloads may converge on the same immutable cache file.
    temporary
        .persist(&path)
        .map_err(|error| ClientError::Io(error.error))?;
    Ok(path)
}

#[cfg(test)]
#[path = "download_tests.rs"]
mod tests;

async fn checksum(path: &Path) -> Result<String> {
    let metadata = tokio::fs::symlink_metadata(path).await?;
    if !metadata.is_file() || metadata.len() > 512 * 1024 * 1024 {
        return Err(ClientError::Integrity);
    }
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
