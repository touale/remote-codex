use crate::{Checked, Result};
use remote_codex_protocol::Fault;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub fn private(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .checked("IO_ERROR", "cannot create private service directory")?;
    let meta =
        fs::symlink_metadata(path).checked("IO_ERROR", "cannot inspect service directory")?;
    if !meta.is_dir()
        || meta.uid() != nix::unistd::geteuid().as_raw()
        || meta.permissions().mode() & 0o077 != 0
    {
        return Err(Fault::new(
            "INSECURE_PATH",
            "service directory must be private and owned by the SSH user",
        ));
    }
    Ok(())
}

pub fn file(path: &Path) -> Result<fs::File> {
    let opened = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path);
    if let Err(error) = &opened
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(Fault::new("IO_ERROR", "cannot create private file"));
    }
    let meta = fs::symlink_metadata(path).checked("IO_ERROR", "cannot inspect private file")?;
    if !meta.is_file()
        || meta.uid() != nix::unistd::geteuid().as_raw()
        || meta.permissions().mode() & 0o077 != 0
    {
        return Err(Fault::new(
            "INSECURE_PATH",
            "service file must be private, regular and owned by the SSH user",
        ));
    }
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .checked("IO_ERROR", "cannot open private file")
}

pub fn socket(root: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .checked("IO_ERROR", "service root does not exist")?;
    let hash = format!("{:x}", Sha256::digest(root.as_os_str().as_encoded_bytes()));
    let directory = PathBuf::from(format!("/tmp/remote-codex-{}", nix::unistd::geteuid()));
    private(&directory)?;
    Ok(directory.join(format!("{}.sock", &hash[..24])))
}
