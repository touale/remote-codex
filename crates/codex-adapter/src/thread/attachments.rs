//! Host filesystem access needed by the native goal composer, never remote execution.
use super::settings::known_fields;
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::path::{Component, Path};

#[cfg(test)]
#[path = "attachments_tests.rs"]
mod tests;

/// Descriptor-relative opens prevent a replaced directory or link escaping the home.
pub(super) fn read_text(home: &Path, path: &str, limit: usize) -> Result<String, Fault> {
    use nix::{
        fcntl::{OFlag, open, openat},
        sys::stat::Mode,
    };
    use std::io::Read;
    let mut params = json!({"path":path});
    prepare("fs/readFile", &mut params, home)?;
    let root = home.canonicalize().map_err(|_| denied())?;
    let path = Path::new(params["path"].as_str().ok_or_else(denied)?);
    let mut parts = path
        .strip_prefix(&root)
        .map_err(|_| denied())?
        .components()
        .peekable();
    let flags = OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK;
    let mut fd = open(&root, flags | OFlag::O_DIRECTORY, Mode::empty()).map_err(|_| denied())?;
    while let Some(part) = parts.next() {
        fd = openat(
            &fd,
            Path::new(part.as_os_str()),
            if parts.peek().is_some() {
                flags | OFlag::O_DIRECTORY
            } else {
                flags
            },
            Mode::empty(),
        )
        .map_err(|_| denied())?;
    }
    let file = std::fs::File::from(fd);
    if !file.metadata().map_err(|_| denied())?.is_file() {
        return Err(denied());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Fault::new("ATTACHMENT_IO", "Could not read goal attachment text."))?;
    if bytes.len() > limit {
        return Err(Fault::new(
            "ATTACHMENT_LIMIT",
            "Goal attachment text exceeds 1 MiB. Split the goal into smaller tasks.",
        ));
    }
    String::from_utf8(bytes).map_err(|_| {
        Fault::new(
            "ATTACHMENT_ENCODING",
            "Goal attachments must contain UTF-8 text.",
        )
    })
}

pub(super) fn prepare(method: &str, params: &mut Value, home: &Path) -> Result<(), Fault> {
    known_fields(
        params,
        match method {
            "fs/createDirectory" => &["path", "recursive"],
            "fs/writeFile" => &["path", "dataBase64"],
            "fs/readFile" => &["path"],
            _ => return Err(denied()),
        },
    )?;
    let path = Path::new(params["path"].as_str().ok_or_else(denied)?);
    let canonical = home.canonicalize().map_err(|_| denied())?;
    let relative = path
        .strip_prefix(home.join("attachments"))
        .or_else(|_| path.strip_prefix(canonical.join("attachments")))
        .map_err(|_| denied())?;
    let mut components = relative.components();
    let Some(Component::Normal(id)) = components.next() else {
        return Err(denied());
    };
    let id = id.to_str().ok_or_else(denied)?;
    let uuid = uuid::Uuid::parse_str(id).map_err(|_| denied())?;
    if uuid.hyphenated().to_string() != id
        || components.any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(denied());
    }
    if method != "fs/createDirectory" && relative.components().count() < 2 {
        return Err(denied());
    }
    if method == "fs/createDirectory"
        && params
            .get("recursive")
            .is_some_and(|v| !v.is_null() && !v.is_boolean())
    {
        return Err(denied());
    }
    if method == "fs/writeFile" && !params["dataBase64"].is_string() {
        return Err(denied());
    }
    // Refuse linked attachment roots and descendants, including dangling links.
    let mut resolved = canonical;
    for component in Path::new("attachments").join(relative).components() {
        resolved.push(component);
        match std::fs::symlink_metadata(&resolved) {
            Ok(meta) if meta.file_type().is_symlink() => return Err(denied()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(denied()),
        }
    }
    params["path"] = json!(resolved);
    Ok(())
}

fn denied() -> Fault {
    Fault::new(
        "ATTACHMENT_PATH_DENIED",
        "The native composer can only access files in this local Codex home's attachment directories.",
    )
}
