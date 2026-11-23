use super::{Endpoint, Result, fault, io};
use cap_std::fs::Dir;
#[cfg(unix)]
use rustix::fs::{Mode, OFlags};
use std::{
    fs::File,
    path::{Component, Path},
};

pub(super) fn components(path: &str) -> Result<Vec<&str>> {
    if path.contains('\0') || path.len() > 4096 {
        return Err(fault("TRANSFER_PATH", "Invalid transfer path."));
    }
    #[cfg(windows)]
    for part in path.split('/') {
        windows_name(part)?;
    }
    Path::new(path)
        .components()
        .map(|c| match c {
            Component::Normal(s) => s
                .to_str()
                .ok_or_else(|| fault("TRANSFER_PATH", "File name is not UTF-8.")),
            _ => Err(fault(
                "TRANSFER_PATH",
                "Transfer path escapes the selected directory.",
            )),
        })
        .collect()
}
#[cfg(unix)]
pub(super) fn directory(dir: &Dir, name: &str) -> Result<Dir> {
    let fd = rustix::fs::openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io)?;
    Ok(Dir::from_std_file(File::from(fd)))
}
#[cfg(unix)]
pub(super) fn file(dir: &Dir, name: &str, access: Access) -> Result<File> {
    let flags = match access {
        Access::Read => OFlags::RDONLY,
        Access::ReadWrite => OFlags::RDWR,
        Access::Create => OFlags::RDWR | OFlags::CREATE,
        Access::Truncate => OFlags::RDWR | OFlags::CREATE | OFlags::TRUNC,
    };
    let fd = rustix::fs::openat(
        dir,
        name,
        flags | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(io)?;
    let file = File::from(fd);
    if !file.metadata().map_err(io)?.is_file() {
        return Err(fault(
            "TRANSFER_TYPE",
            "Only regular files can be transferred.",
        ));
    }
    Ok(file)
}
impl Endpoint {
    pub(super) fn directory(&self, path: &str) -> Result<Dir> {
        let mut dir = self.root.try_clone().map_err(io)?;
        for name in components(path)? {
            dir = directory(&dir, name)?;
        }
        Ok(dir)
    }
    pub(super) fn parent(&self, path: &str) -> Result<(Dir, String)> {
        let mut parts = components(path)?;
        let name = parts
            .pop()
            .ok_or_else(|| fault("TRANSFER_PATH", "A file name is required."))?
            .to_owned();
        let mut dir = self.root.try_clone().map_err(io)?;
        for part in parts {
            dir = directory(&dir, part)?;
        }
        Ok((dir, name))
    }
}

#[derive(Clone, Copy)]
pub(super) enum Access {
    Read,
    ReadWrite,
    Create,
    Truncate,
}

#[cfg(windows)]
fn windows_name(name: &str) -> Result<()> {
    let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\\|?*".contains(c))
        || matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((base.starts_with("COM") || base.starts_with("LPT"))
            && base.len() == 4
            && base.as_bytes()[3].is_ascii_digit())
    {
        return Err(fault(
            "TRANSFER_NAME",
            "This name cannot be represented safely on Windows.",
        ));
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn directory(dir: &Dir, name: &str) -> Result<Dir> {
    use cap_fs_ext::{DirExt, OsMetadataExt};
    let child = dir.open_dir_nofollow(name).map_err(io)?;
    if child.dir_metadata().map_err(io)?.file_attributes() & 0x400 != 0 {
        return Err(fault(
            "TRANSFER_PATH",
            "Reparse points cannot be traversed.",
        ));
    }
    Ok(child)
}

#[cfg(windows)]
pub(super) fn file(dir: &Dir, name: &str, access: Access) -> Result<File> {
    use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
    use cap_std::fs::OpenOptions;
    // Refuse a differently-cased destination rather than overwrite it on NTFS.
    for entry in dir.entries().map_err(io)? {
        let actual = entry.map_err(io)?.file_name();
        if let Some(actual) = actual.to_str()
            && actual != name
            && actual.to_lowercase() == name.to_lowercase()
        {
            return Err(fault(
                "TRANSFER_NAME_COLLISION",
                "A destination differs only in letter case.",
            ));
        }
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    match access {
        Access::Read => {}
        Access::ReadWrite => {
            options.write(true);
        }
        Access::Create => {
            options.write(true).create(true);
        }
        Access::Truncate => {
            options.write(true).create(true).truncate(true);
        }
    }
    let file = dir.open_with(name, &options).map_err(io)?.into_std();
    let metadata = file.metadata().map_err(io)?;
    if !metadata.is_file() || remote_codex_platform::fs::is_link(&metadata) {
        return Err(fault(
            "TRANSFER_TYPE",
            "Only regular files can be transferred.",
        ));
    }
    Ok(file)
}

pub(super) fn protect_stage(parent: &Dir, name: &str, dir: &Dir) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = (parent, name);
        dir.set_permissions(
            ".",
            cap_std::fs::Permissions::from_std(std::fs::Permissions::from_mode(0o700)),
        )
        .map_err(io)
    }
    #[cfg(windows)]
    {
        use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
        use cap_std::fs::{OpenOptions, OpenOptionsExt};
        let _ = dir;
        let mut options = OpenOptions::new();
        options
            .access_mode(0x00040000 | 0x80000000)
            .custom_flags(0x02000000)
            .follow(FollowSymlinks::No);
        let file = parent.open_with(name, &options).map_err(io)?.into_std();
        remote_codex_platform::fs::protect(&file).map_err(io)
    }
}
