use super::fs::{self, Creation, FileLock, LockMode};

#[test]
fn shared_locks_exclude_writers_until_all_readers_release() -> std::io::Result<()> {
    let root = tempfile::tempdir()?;
    let directory = root.path().join("state");
    fs::private_dir(&directory)?;
    let path = directory.join("lock");
    let first = FileLock::try_lock(
        fs::open_private(&path, Creation::IfMissing)?,
        LockMode::Shared,
    )
    .map_err(|(_, e)| std::io::Error::from(e))?;
    let second = FileLock::try_lock(
        fs::open_private(&path, Creation::Existing)?,
        LockMode::Shared,
    )
    .map_err(|(_, e)| std::io::Error::from(e))?;
    assert!(
        FileLock::try_lock(
            fs::open_private(&path, Creation::Existing)?,
            LockMode::Exclusive
        )
        .is_err()
    );
    drop(first);
    assert!(
        FileLock::try_lock(
            fs::open_private(&path, Creation::Existing)?,
            LockMode::Exclusive
        )
        .is_err()
    );
    drop(second);
    assert!(
        FileLock::try_lock(
            fs::open_private(&path, Creation::Existing)?,
            LockMode::Exclusive
        )
        .is_ok()
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn private_storage_uses_user_acl_and_rejects_reparse_points() -> std::io::Result<()> {
    let root = tempfile::tempdir()?;
    let state = root.path().join("state");
    fs::private_dir(&state)?;
    let path = state.join("secret");
    let file = fs::open_private(&path, Creation::New)?;
    fs::check_private(&path)?;
    assert_eq!(fs::identity(&file)?, fs::identity_at(&path)?);
    let link = root.path().join("linked");
    let status = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&link)
        .arg(&state)
        .output()?;
    assert!(status.status.success());
    assert!(fs::check_private(&link).is_err());
    std::fs::remove_dir(link)?;
    Ok(())
}
