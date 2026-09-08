use crate::{Checked, Result, paths};
use remote_codex_protocol::{Fault, SkillFile};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    os::unix::fs::PermissionsExt,
    path::{Component, Path},
};
use tokio::io::AsyncReadExt;

const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) async fn prepare(root: &Path, digest: &str, files: &[SkillFile]) -> Result<Value> {
    validate(digest, files)?;
    let cache = root.join("skills");
    paths::private(&cache)?;
    let target = cache.join(digest);
    if target.exists() {
        verify(&target, files).await?;
        return Ok(json!({"ready":true,"path":target}));
    }
    let staging = root.join("skill-staging");
    paths::private(&staging)?;
    let mut entries = tokio::fs::read_dir(&staging)
        .await
        .checked("SKILL_IO", "cannot inspect skill staging")?;
    let mut active = 0;
    while let Some(entry) = entries
        .next_entry()
        .await
        .checked("SKILL_IO", "cannot inspect skill staging")?
    {
        let metadata = entry
            .metadata()
            .await
            .checked("SKILL_IO", "cannot inspect skill stage")?;
        if metadata
            .modified()
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 3600)
            && metadata.is_dir()
        {
            let _ = tokio::fs::remove_dir_all(entry.path()).await;
        } else {
            active += 1;
        }
    }
    if active >= 32 {
        return Err(Fault::new(
            "SKILL_CAPACITY",
            "too many pending skill uploads",
        ));
    }
    let stage = uuid::Uuid::new_v4().to_string();
    let directory = staging.join(&stage);
    paths::private(&directory)?;
    let assets = directory.join("assets");
    paths::private(&assets)?;
    for file in files {
        if let Some(parent) = assets.join(&file.path).parent() {
            paths::private(parent)?;
        }
    }
    let manifest = serde_json::to_vec(files).checked("SKILL_MANIFEST", "invalid skill manifest")?;
    tokio::fs::write(directory.join("manifest.json"), manifest)
        .await
        .checked("SKILL_IO", "cannot persist skill manifest")?;
    Ok(json!({"ready":false,"stage":stage,"path":assets}))
}

pub(crate) async fn commit(root: &Path, stage: &str, digest: &str) -> Result<Value> {
    uuid::Uuid::parse_str(stage)
        .map_err(|_| Fault::new("SKILL_MANIFEST", "invalid skill stage"))?;
    let directory = root.join("skill-staging").join(stage);
    let bytes = tokio::fs::read(directory.join("manifest.json"))
        .await
        .checked("SKILL_IO", "skill stage unavailable")?;
    if bytes.len() > 256 * 1024 {
        return Err(Fault::new("SKILL_MANIFEST", "skill manifest too large"));
    }
    let files: Vec<SkillFile> =
        serde_json::from_slice(&bytes).checked("SKILL_MANIFEST", "invalid skill manifest")?;
    validate(digest, &files)?;
    let assets = directory.join("assets");
    verify(&assets, &files).await?;
    for file in &files {
        tokio::fs::set_permissions(
            assets.join(&file.path),
            std::fs::Permissions::from_mode(if file.executable { 0o700 } else { 0o600 }),
        )
        .await
        .checked("SKILL_IO", "cannot set skill permissions")?;
    }
    let target = root.join("skills").join(digest);
    match tokio::fs::rename(&assets, &target).await {
        Ok(()) => {}
        Err(_) if target.exists() => verify(&target, &files).await?,
        Err(_) => return Err(Fault::new("SKILL_IO", "cannot publish skill resources")),
    }
    let _ = tokio::fs::remove_dir_all(directory).await;
    Ok(json!({"ready":true,"path":target}))
}

fn validate(digest: &str, files: &[SkillFile]) -> Result<()> {
    let invalid = || Fault::new("SKILL_MANIFEST", "invalid or oversized skill manifest");
    if files.is_empty() || files.len() > 500 || !files.iter().any(|f| f.path == "SKILL.md") {
        return Err(invalid());
    }
    let bytes = serde_json::to_vec(files).checked("SKILL_MANIFEST", "invalid skill manifest")?;
    if digest != format!("{:x}", Sha256::digest(&bytes)) {
        return Err(invalid());
    }
    let mut paths = HashSet::new();
    let mut total = 0_u64;
    for file in files {
        if file.path.is_empty()
            || file.path.len() > 1024
            || file.path.chars().any(char::is_control)
            || Path::new(&file.path)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || !paths.insert(&file.path)
            || file.size > MAX_BYTES
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid());
        }
        total += file.size;
        if total > MAX_BYTES {
            return Err(invalid());
        }
    }
    Ok(())
}

async fn verify(root: &Path, files: &[SkillFile]) -> Result<()> {
    for file in files {
        let path = root.join(&file.path);
        let metadata = tokio::fs::symlink_metadata(&path)
            .await
            .checked("SKILL_INTEGRITY", "skill file missing")?;
        if !metadata.is_file() || metadata.len() != file.size {
            return Err(Fault::new("SKILL_INTEGRITY", "skill file changed"));
        }
        let canonical = tokio::fs::canonicalize(&path)
            .await
            .checked("SKILL_INTEGRITY", "invalid skill file")?;
        if !canonical.starts_with(
            root.canonicalize()
                .checked("SKILL_INTEGRITY", "invalid skill root")?,
        ) {
            return Err(Fault::new(
                "SKILL_INTEGRITY",
                "skill path escapes its directory",
            ));
        }
        let mut input = tokio::fs::File::open(&path)
            .await
            .checked("SKILL_IO", "cannot read skill resource")?
            .take(MAX_BYTES + 1);
        let mut bytes = Vec::new();
        input
            .read_to_end(&mut bytes)
            .await
            .checked("SKILL_IO", "cannot read skill resource")?;
        if format!("{:x}", Sha256::digest(bytes)) != file.sha256 {
            return Err(Fault::new("SKILL_INTEGRITY", "skill checksum mismatch"));
        }
    }
    Ok(())
}
