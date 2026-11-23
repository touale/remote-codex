use super::{UpdateComponent, error};
use crate::Result;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

pub(super) fn extract(archive: &Path, directory: &Path, kind: UpdateComponent) -> Result<PathBuf> {
    if kind != UpdateComponent::Cli {
        return Err(error("UPDATE_ARCHIVE", "Expected a CLI package."));
    }
    let mut archive = zip::ZipArchive::new(File::open(archive)?)
        .map_err(|_| error("UPDATE_ARCHIVE", "Invalid update ZIP."))?;
    if archive.len() != 1 {
        return Err(error(
            "UPDATE_ARCHIVE",
            "Unexpected update archive entries.",
        ));
    }
    let mut entry = archive
        .by_index(0)
        .map_err(|_| error("UPDATE_ARCHIVE", "Missing update executable."))?;
    if entry
        .name()
        .map_err(|_| error("UPDATE_ARCHIVE", "Invalid archive name."))?
        != "remote-codex.exe"
        || entry.is_symlink()
        || entry.size() > 512 * 1024 * 1024
    {
        return Err(error("UPDATE_ARCHIVE", "Invalid update executable."));
    }
    let path = directory.join("remote-codex.exe");
    let mut file =
        remote_codex_platform::fs::open_private(&path, remote_codex_platform::fs::Creation::New)?;
    std::io::copy(&mut entry, &mut file)?;
    file.sync_all()?;
    Ok(path)
}
