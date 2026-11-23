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
    use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
    use cap_std::fs::{Dir, OpenOptions};
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
    let mut dir =
        Dir::open_ambient_dir(&root, cap_std::ambient_authority()).map_err(|_| denied())?;
    let mut file = None;
    while let Some(part) = parts.next() {
        if parts.peek().is_some() {
            dir = dir
                .open_dir_nofollow(Path::new(part.as_os_str()))
                .map_err(|_| denied())?;
        } else {
            let mut options = OpenOptions::new();
            options.read(true).follow(FollowSymlinks::No);
            file = Some(
                dir.open_with(Path::new(part.as_os_str()), &options)
                    .map_err(|_| denied())?
                    .into_std(),
            );
        }
    }
    let file = file.ok_or_else(denied)?;
    if remote_codex_platform::fs::is_link(&file.metadata().map_err(|_| denied())?) {
        return Err(denied());
    }
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
            Ok(meta) if remote_codex_platform::fs::is_link(&meta) => return Err(denied()),
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
