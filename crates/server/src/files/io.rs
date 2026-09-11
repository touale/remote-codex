use crate::{Checked, Result};
use cap_std::fs::{Dir, OpenOptions, OpenOptionsExt};
use remote_codex_protocol::{DirectoryEntry, DirectoryPage, Fault, MAX_TEXT_FILE};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
};

pub(super) fn relative(path: &str, allow_root: bool) -> Result<PathBuf> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|p| matches!(p, Component::ParentDir | Component::Prefix(_)))
        || path
            .as_os_str()
            .to_string_lossy()
            .chars()
            .any(char::is_control)
        || (!allow_root && (path.as_os_str().is_empty() || path == Path::new(".")))
    {
        return Err(Fault::new(
            "INVALID_FILE_PATH",
            "Choose a path inside this workspace.",
        ));
    }
    Ok(if path.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        path.into()
    })
}

pub(super) fn read(dir: &Dir, path: &Path) -> Result<(Vec<u8>, String)> {
    let file = dir
        .open_with(
            path,
            OpenOptions::new()
                .read(true)
                .custom_flags(nix::libc::O_NONBLOCK),
        )
        .checked("FILE_READ", "Cannot open this workspace file.")?;
    let meta = file
        .metadata()
        .checked("FILE_READ", "Cannot inspect this file.")?;
    if !meta.is_file() || meta.len() > MAX_TEXT_FILE as u64 {
        return Err(Fault::new(
            "FILE_TOO_LARGE",
            "The editor supports regular UTF-8 files up to 4 MiB.",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_TEXT_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .checked("FILE_READ", "Cannot read this file.")?;
    if bytes.len() > MAX_TEXT_FILE {
        return Err(Fault::new("FILE_TOO_LARGE", "File exceeds 4 MiB."));
    }
    let revision = format!("{:x}", Sha256::digest(&bytes));
    Ok((bytes, revision))
}

pub(super) fn revision(dir: &Dir, path: &Path) -> Result<Option<String>> {
    match dir.symlink_metadata(path) {
        Ok(_) => Ok(Some(read(dir, path)?.1)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(Fault::new(
            "FILE_READ",
            "Cannot inspect the destination file.",
        )),
    }
}

pub(super) fn list(dir: &Dir, path: &str) -> Result<DirectoryPage> {
    let relative = relative(path, true)?;
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut encoded_size = 0;
    for entry in dir
        .read_dir(&relative)
        .checked("DIRECTORY_READ", "Cannot list this directory.")?
    {
        if entries.len() == 2000 {
            truncated = true;
            break;
        }
        let entry = entry.checked("DIRECTORY_READ", "Cannot inspect a directory entry.")?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.chars().any(char::is_control) {
            continue;
        }
        let child = relative.join(&name);
        let metadata = dir
            .symlink_metadata(&child)
            .checked("DIRECTORY_READ", "Cannot inspect a directory entry.")?;
        let resolved = dir.metadata(&child).ok();
        let entry = DirectoryEntry {
            name,
            path: child.to_string_lossy().trim_start_matches("./").into(),
            directory: resolved.as_ref().is_some_and(|m| m.is_dir()),
            symlink: metadata.is_symlink(),
            size: metadata.len(),
        };
        // JSON escapes and long parent paths count toward the wire budget too.
        encoded_size += serde_json::to_vec(&entry)
            .map_err(|_| Fault::new("DIRECTORY_READ", "Cannot encode directory entry."))?
            .len()
            + 1;
        if encoded_size > 512 * 1024 {
            truncated = true;
            break;
        }
        entries.push(entry);
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(DirectoryPage { entries, truncated })
}
