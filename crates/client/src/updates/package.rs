use super::{
    UpdateComponent, error,
    installation::{self, Installation},
    release::{self, Asset, Release},
};
use crate::Result;
use base64::Engine;
use std::{
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

#[derive(serde::Serialize)]
pub struct UpdateProgress {
    pub phase: String,
    pub received: u64,
    pub total: u64,
}

async fn download(
    root: &Path,
    asset: &Asset,
    report: &(dyn Fn(UpdateProgress) + Send + Sync),
) -> Result<tempfile::NamedTempFile> {
    let client = release::client()?;
    let signature = release::small(&client, &asset.signature, 16_384).await?;
    let mut response = client.get(&asset.url).send().await?.error_for_status()?;
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    let mut received = 0u64;
    let mut tick = std::time::Instant::now();
    while let Some(chunk) = response.chunk().await? {
        received = received.saturating_add(chunk.len() as u64);
        if received > asset.size {
            return Err(error(
                "UPDATE_SIGNATURE",
                "The update package exceeds its declared size.",
            ));
        }
        file.write_all(&chunk)?;
        if tick.elapsed().as_millis() >= 250 {
            report(UpdateProgress {
                phase: "downloading".into(),
                received,
                total: asset.size,
            });
            tick = std::time::Instant::now();
        }
    }
    if received != asset.size {
        return Err(error(
            "UPDATE_SIGNATURE",
            "The update download is incomplete.",
        ));
    }
    file.as_file().sync_all()?;
    report(UpdateProgress {
        phase: "verifying".into(),
        received,
        total: asset.size,
    });
    verify(file.path(), &signature, include_str!("public.key"))?;
    Ok(file)
}

pub(super) fn verify(path: &Path, signature: &[u8], public_key: &str) -> Result<()> {
    let invalid = || {
        error(
            "UPDATE_SIGNATURE",
            "Remote Codex update signature verification failed.",
        )
    };
    let decode = |bytes: &[u8]| -> Result<String> {
        let value = base64::prelude::BASE64_STANDARD
            .decode(
                bytes
                    .iter()
                    .copied()
                    .filter(|b| !b.is_ascii_whitespace())
                    .collect::<Vec<_>>(),
            )
            .map_err(|_| invalid())?;
        String::from_utf8(value).map_err(|_| invalid())
    };
    let key = minisign_verify::PublicKey::decode(&decode(public_key.as_bytes())?)
        .map_err(|_| invalid())?;
    let signature =
        minisign_verify::Signature::decode(&decode(signature)?).map_err(|_| invalid())?;
    let mut verifier = key.verify_stream(&signature).map_err(|_| invalid())?;
    let mut file = File::open(path)?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        verifier.update(&buffer[..count]);
    }
    verifier.finalize().map_err(|_| invalid())
}

pub(super) fn extract(archive: &Path, directory: &Path, kind: UpdateComponent) -> Result<PathBuf> {
    let expected = match kind {
        UpdateComponent::Cli => "remote-codex",
        UpdateComponent::App => "Remote Codex.app",
    };
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?));
    let mut size = 0u64;
    let mut links = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let parts: Vec<_> = path
            .components()
            .filter(|c| *c != Component::CurDir)
            .collect();
        if parts.first() != Some(&Component::Normal(expected.as_ref()))
            || parts.iter().any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(error(
                "UPDATE_ARCHIVE",
                "An update package contains an unsafe path.",
            ));
        }
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir() || entry_type.is_symlink()) {
            return Err(error(
                "UPDATE_ARCHIVE",
                "An update package contains an unsupported entry.",
            ));
        }
        if let Some(link) = entry.link_name()? {
            links.push(directory.join(&path));
            let mut depth = parts.len().saturating_sub(1);
            for component in link.components() {
                match component {
                    Component::Normal(_) => depth += 1,
                    Component::CurDir => {}
                    Component::ParentDir if depth > 1 => depth -= 1,
                    _ => {
                        return Err(error(
                            "UPDATE_ARCHIVE",
                            "An update package contains an unsafe link.",
                        ));
                    }
                }
            }
        }
        size = size.saturating_add(entry.size());
        if size > 2 * 1024 * 1024 * 1024 || !entry.unpack_in(directory)? {
            return Err(error(
                "UPDATE_ARCHIVE",
                "The update package cannot be safely extracted.",
            ));
        }
    }
    let path = directory.join(expected);
    if std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(error(
            "UPDATE_ARCHIVE",
            "The update root must not be a link.",
        ));
    }
    let canonical = path.canonicalize()?;
    for link in links {
        if !link.canonicalize()?.starts_with(&canonical) {
            return Err(error(
                "UPDATE_ARCHIVE",
                "An update link resolves outside its component.",
            ));
        }
    }
    Ok(path)
}

pub(super) async fn install(
    root: &Path,
    target: &Installation,
    release: &Release,
    report: &(dyn Fn(UpdateProgress) + Send + Sync),
) -> Result<()> {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return Err(error(
            "UPDATE_PLATFORM",
            "In-app updates are available for macOS Apple Silicon releases.",
        ));
    }
    let original_metadata = std::fs::symlink_metadata(&target.path)?;
    let installed_version = installation::read_version(target.component, &target.path).await?;
    if !release::newer(&release.info.version, &installed_version) {
        return Ok(());
    }
    let asset = release.assets.get(&target.component).ok_or_else(|| {
        error(
            "UPDATE_UNAVAILABLE",
            "This release has no signed update package for this program.",
        )
    })?;
    let parent = target
        .path
        .parent()
        .ok_or_else(|| error("UPDATE_PATH", "Invalid installation path."))?;
    let stage = tempfile::Builder::new()
        .prefix(".remote-codex-update-")
        .tempdir_in(parent)?;
    report(UpdateProgress {
        phase: "downloading".into(),
        received: 0,
        total: asset.size,
    });
    let file = download(root, asset, report).await?;
    let path = extract(file.path(), stage.path(), target.component)?;
    if installation::read_version(target.component, &path).await? != release.info.version {
        return Err(error(
            "UPDATE_IDENTITY",
            "The signed package does not match the release version.",
        ));
    }
    report(UpdateProgress {
        phase: "installing".into(),
        received: asset.size,
        total: asset.size,
    });
    #[cfg(target_os = "macos")]
    {
        let status = tokio::process::Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&path)
            .output()
            .await?;
        if !status.status.success() {
            return Err(error(
                "UPDATE_IDENTITY",
                "The update has an invalid macOS code signature.",
            ));
        }
        replace(&path, &target.path, &original_metadata)?;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = original_metadata;
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn replace(staged: &Path, target: &Path, original: &std::fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let current = std::fs::symlink_metadata(target)?;
    if current.ino() != original.ino() || current.dev() != original.dev() {
        return Err(error(
            "UPDATE_CHANGED",
            "The installation was replaced during download. Check again.",
        ));
    }
    // Swapping whole inodes also supports app bundles and keeps running images alive.
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        staged,
        rustix::fs::CWD,
        target,
        rustix::fs::RenameFlags::EXCHANGE,
    )
    .map_err(std::io::Error::from)?;
    Ok(())
}
