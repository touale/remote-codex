use crate::progress::{PrepareEvent, PrepareStage, TransferKind, TransferProgress};
use crate::{
    ClientError, Result,
    protocol::{Request, SkillFile},
    remote::Remote,
};
use remote_codex_adapter::thread::Codex;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

/// Maps session Skill paths to verified assets in the selected environment.
#[derive(Clone, Default)]
pub(crate) struct SkillMap {
    mappings: BTreeMap<String, String>,
    digests: BTreeMap<String, String>,
}

struct Bundle {
    root: PathBuf,
    files: Vec<SkillFile>,
    digest: String,
}

impl SkillMap {
    pub(crate) async fn prepare(
        engine: &Codex,
        remote: &Remote,
        home: &Path,
        progress: &(dyn Fn(PrepareEvent) + Send + Sync),
        expected: Option<&Self>,
    ) -> Result<Self> {
        progress(PrepareEvent::Stage(PrepareStage::PrepareSkills));
        let bundles = scan(engine.enabled_skills(home).await?, expected)?;
        let mut map = Self::default();
        for (path, bundle) in bundles {
            let Bundle {
                root,
                files,
                digest,
            } = bundle;
            let prepared = remote
                .call(Request::PrepareSkill {
                    digest: digest.clone(),
                    files: files.clone(),
                })
                .await?;
            let destination = prepared["path"]
                .as_str()
                .ok_or(ClientError::RemoteResponse)?;
            let installed = if prepared["ready"] == true {
                destination.to_owned()
            } else {
                progress(PrepareEvent::Stage(PrepareStage::PrepareSkills));
                let total = files.iter().map(|f| f.size).sum();
                let mut completed = 0;
                for file in &files {
                    remote
                        .ssh
                        .upload(
                            &remote.server.endpoint,
                            &root.join(&file.path),
                            &format!("{destination}/{}", file.path),
                            |bytes, _| {
                                progress(PrepareEvent::Transfer(TransferProgress {
                                    kind: TransferKind::Upload,
                                    transferred_bytes: completed + bytes,
                                    total_bytes: Some(total),
                                }))
                            },
                        )
                        .await?;
                    completed += file.size;
                }
                let result = remote
                    .call(Request::CommitSkill {
                        stage: prepared["stage"]
                            .as_str()
                            .ok_or(ClientError::RemoteResponse)?
                            .into(),
                        digest: digest.clone(),
                    })
                    .await?;
                result["path"]
                    .as_str()
                    .ok_or(ClientError::RemoteResponse)?
                    .into()
            };
            map.mappings
                .insert(path.clone(), format!("{installed}/SKILL.md"));
            map.digests.insert(path, digest);
        }
        Ok(map)
    }

    pub(crate) fn mappings(&self) -> &BTreeMap<String, String> {
        &self.mappings
    }

    pub(crate) fn instructions(&self) -> String {
        if self.mappings.is_empty() {
            return String::new();
        }
        let mappings = self
            .mappings
            .iter()
            .map(|(local, remote)| format!("{} => {}", json!(local), json!(remote)))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Local Skill resources prepared for this session are available in the remote environment. These mappings do not enable Skills; Codex controls the enabled catalog. For instructions and scripts from these skills, resolve relative resources against the corresponding remote directory. Do not install dependencies implicitly.\n{mappings}"
        )
    }
}

// Validate every bound resource locally before preparing any remote uploads.
fn scan(paths: Vec<PathBuf>, expected: Option<&SkillMap>) -> Result<BTreeMap<String, Bundle>> {
    let mut paths: BTreeSet<PathBuf> = paths.into_iter().collect();
    // Plugin discovery can finish after skills/list has returned. Retain
    // resources already referenced by this session even if a fresh catalog
    // temporarily omits them; Codex still controls which Skills are enabled.
    if let Some(expected) = expected {
        paths.extend(expected.digests.keys().map(PathBuf::from));
    }
    let mut bundles = BTreeMap::new();
    for path in paths {
        if !path.is_absolute() || !path.is_file() {
            continue;
        }
        let Some(root) = path.parent() else {
            continue;
        };
        let root = root.canonicalize()?;
        let files = manifest(&root)?;
        let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&files)?));
        bundles.insert(
            path.to_string_lossy().into_owned(),
            Bundle {
                root,
                files,
                digest,
            },
        );
    }
    if let Some(expected) = expected {
        for (path, digest) in &expected.digests {
            if bundles
                .get(path)
                .is_none_or(|bundle| &bundle.digest != digest)
            {
                return Err(remote_codex_protocol::Fault::new(
                    "SKILLS_CHANGED",
                    &format!(
                        "previously prepared Skill resources changed or became unavailable at {}; exit and resume this session to apply the current resources",
                        json!(path)
                    ),
                )
                .into());
            }
        }
    }
    Ok(bundles)
}

fn manifest(root: &Path) -> Result<Vec<SkillFile>> {
    let mut stack = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut total = 0;
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if excluded(&name) {
                continue;
            }
            // Symlinked resources are not copied, including links within the root:
            // the bundle's behavior must not depend on a link changing mid-upload.
            if metadata.file_type().is_symlink() {
                return Err(ClientError::Argument(
                    "enabled Skill contains a symbolic link; replace it with a regular resource before remote execution",
                ));
            }
            if metadata.is_dir() {
                stack.push(path);
                if stack.len() > 500 {
                    return Err(ClientError::Argument("Skill directory is too large"));
                }
                continue;
            }
            if !metadata.is_file() {
                return Err(ClientError::Argument(
                    "Skill contains a non-regular resource",
                ));
            }
            total += metadata.len();
            if total > 16 * 1024 * 1024 || files.len() >= 500 {
                return Err(ClientError::Argument("Skill exceeds 16 MiB or 500 files"));
            }
            let bytes = std::fs::read(&path)?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| ClientError::PrivatePath)?
                .to_str()
                .ok_or(ClientError::Argument("Skill path must be UTF-8"))?
                .to_owned();
            files.push(SkillFile {
                path: relative,
                size: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
                executable: metadata.permissions().mode() & 0o111 != 0,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn excluded(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | "__pycache__"
            | "auth.json"
            | "credentials.json"
            | ".DS_Store"
    ) || name.starts_with(".env")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
}

#[cfg(test)]
mod tests;
