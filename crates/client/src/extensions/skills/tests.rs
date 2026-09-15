use super::*;
use std::fs;

fn prepared(path: &Path) -> Result<SkillMap> {
    Ok(SkillMap {
        digests: scan(vec![path.to_path_buf()], None)?
            .into_iter()
            .map(|(path, bundle)| (path, bundle.digest))
            .collect(),
        ..SkillMap::default()
    })
}

#[test]
fn recovery_keeps_bound_resources_when_the_catalog_grows_or_omits_them() -> Result<()> {
    let root = tempfile::tempdir()?;
    let original = root.path().join("original/SKILL.md");
    fs::create_dir(original.parent().ok_or(ClientError::PrivatePath)?)?;
    fs::write(&original, "original instructions")?;
    let expected = prepared(&original)?;
    let added = root.path().join("late-plugin/SKILL.md");
    fs::create_dir(added.parent().ok_or(ClientError::PrivatePath)?)?;
    fs::write(&added, "new plugin instructions")?;
    let expanded = scan(vec![original.clone(), added.clone()], Some(&expected))?;
    assert_eq!(expanded.len(), 2);
    let omitted = scan(vec![added], Some(&expected))?;
    assert_eq!(omitted.len(), 2);
    assert_eq!(
        omitted[original.to_str().ok_or(ClientError::PrivatePath)?].digest,
        expanded[original.to_str().ok_or(ClientError::PrivatePath)?].digest
    );
    // Rewriting identical bytes must not turn an mtime change into SKILLS_CHANGED.
    fs::write(&original, "original instructions")?;
    assert_eq!(scan(Vec::new(), Some(&expected))?.len(), 1);
    Ok(())
}

#[test]
fn recovery_rejects_changed_or_missing_bound_resources() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("SKILL.md");
    let script = root.path().join("run.sh");
    fs::write(&path, "instructions")?;
    fs::write(&script, "echo original")?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o644))?;
    let expected = prepared(&path)?;
    let rejected = || {
        assert!(scan(Vec::new(), Some(&expected)).err().is_some_and(|e| {
            e.code() == "SKILLS_CHANGED" && e.to_string().contains(&path.to_string_lossy()[..])
        }));
    };

    fs::write(&script, "echo modified")?;
    rejected();
    fs::write(&script, "echo original")?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755))?;
    rejected();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o644))?;
    fs::remove_file(&script)?;
    rejected();
    fs::write(&script, "echo original")?;
    fs::set_permissions(&script, fs::Permissions::from_mode(0o644))?;
    fs::remove_file(&path)?;
    rejected();
    Ok(())
}

#[test]
fn bundle_rejects_links_and_omits_credential_files() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("SKILL.md"), "instructions")?;
    fs::write(root.path().join(".env"), "secret")?;
    assert_eq!(manifest(root.path())?.len(), 1);
    std::os::unix::fs::symlink("/etc/passwd", root.path().join("outside"))?;
    assert!(manifest(root.path()).is_err());
    Ok(())
}
