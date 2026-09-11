use super::{Endpoint, Result, fault, io};
use cap_std::fs::Dir;
use rustix::fs::{Mode, OFlags};
use std::{
    fs::File,
    path::{Component, Path},
};

pub(super) fn components(path: &str) -> Result<Vec<&str>> {
    if path.contains('\0') || path.len() > 4096 {
        return Err(fault("TRANSFER_PATH", "Invalid transfer path."));
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
pub(super) fn file(dir: &Dir, name: &str, flags: OFlags) -> Result<File> {
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
