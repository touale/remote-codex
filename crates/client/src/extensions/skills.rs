use crate::progress::{PrepareEvent, PrepareStage, TransferKind, TransferProgress};
use crate::{
    ClientError, Result,
    protocol::{Request, SkillFile},
    remote::Remote,
};
use remote_codex_adapter::thread::Codex;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, os::unix::fs::PermissionsExt, path::Path};

/// Maps enabled local Skill paths to verified assets in the selected environment.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct SkillMap(BTreeMap<String, String>);

impl SkillMap {
    pub(crate) async fn prepare(
        engine: &Codex,
        remote: &Remote,
        home: &Path,
        progress: &(dyn Fn(PrepareEvent) + Send + Sync),
    ) -> Result<Self> {
        progress(PrepareEvent::Stage(PrepareStage::PrepareSkills));
        let mut map = BTreeMap::new();
        for path in engine.enabled_skills(home).await? {
            let path = path.as_path();
            if !path.is_absolute() || !path.is_file() {
                continue;
            }
            let Some(root) = path.parent() else {
                continue;
            };
            let canonical = root.canonicalize()?;
            let files = manifest(&canonical)?;
            let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&files)?));
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
                            &canonical.join(&file.path),
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
                        digest,
                    })
                    .await?;
                result["path"]
                    .as_str()
                    .ok_or(ClientError::RemoteResponse)?
                    .into()
            };
            map.insert(
                path.to_string_lossy().into_owned(),
                format!("{installed}/SKILL.md"),
            );
        }
        Ok(Self(map))
    }

    pub(crate) fn mappings(&self) -> &BTreeMap<String, String> {
        &self.0
    }

    pub(crate) fn instructions(&self) -> String {
        if self.0.is_empty() {
            return String::new();
        }
        let mappings = self
            .0
            .iter()
            .map(|(local, remote)| format!("{} => {}", json!(local), json!(remote)))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Enabled local Skill assets are prepared in the remote environment. For instructions and scripts from these skills, resolve relative resources against the corresponding remote directory. Do not install dependencies implicitly.\n{mappings}"
        )
    }
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
mod tests {
    use super::*;
    #[test]
    fn bundle_rejects_links_and_omits_credential_files() -> Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::write(root.path().join("SKILL.md"), "instructions")?;
        std::fs::write(root.path().join(".env"), "secret")?;
        assert_eq!(manifest(root.path())?.len(), 1);
        std::os::unix::fs::symlink("/etc/passwd", root.path().join("outside"))?;
        assert!(manifest(root.path()).is_err());
        Ok(())
    }
}
