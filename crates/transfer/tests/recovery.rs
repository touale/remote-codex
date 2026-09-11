use remote_codex_protocol::transfer::{CHUNK, Command, Reply, Stamp};
use remote_codex_transfer::{Endpoint, digest};
use std::{os::unix::fs::PermissionsExt, path::Path};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
fn stat(endpoint: &Endpoint, path: &str) -> Result<Stamp> {
    let (reply, _) = endpoint.dispatch(Command::Stat { path: path.into() }, &[])?;
    let Reply::Stat { stamp: Some(stamp) } = reply else {
        return Err("Missing source".into());
    };
    Ok(stamp)
}
fn prepare(
    endpoint: &Endpoint,
    token: &str,
    source: &Stamp,
    expected: Option<Stamp>,
) -> Result<(u64, bool)> {
    let (reply, _) = endpoint.dispatch(
        Command::Prepare {
            path: "target".into(),
            token: token.into(),
            source: source.clone(),
            expected,
        },
        &[],
    )?;
    let Reply::Ready { offset, complete } = reply else {
        return Err("Missing checkpoint".into());
    };
    Ok((offset, complete))
}
fn block(endpoint: &Endpoint, token: &str, offset: u64, bytes: &[u8]) -> Result {
    endpoint.dispatch(
        Command::Write {
            path: "target".into(),
            token: token.into(),
            offset,
            digest: digest(bytes),
        },
        bytes,
    )?;
    Ok(())
}
fn source(root: &Path, content: &[u8]) -> Result<(Endpoint, Stamp)> {
    std::fs::write(root.join("source"), content)?;
    let endpoint = Endpoint::open(root, "owner")?;
    let stamp = stat(&endpoint, "source")?;
    Ok((endpoint, stamp))
}
#[test]
fn binary_transfer_resumes_and_publishes_only_verified_complete_files() -> Result {
    let local = tempfile::tempdir()?;
    let remote = tempfile::tempdir()?;
    let bytes: Vec<u8> = (0..(6 * 1024 * 1024 + 13))
        .map(|n| (n % 251) as u8)
        .collect();
    let (src, mut stamp) = source(local.path(), &bytes)?;
    stamp.mode = 0o755;
    let dst = Endpoint::open(remote.path(), "owner")?;
    let token = uuid::Uuid::new_v4().to_string();
    assert_eq!(prepare(&dst, &token, &stamp, None)?, (0, false));
    block(&dst, &token, 0, &bytes[..CHUNK])?;
    assert!(!remote.path().join("target").exists());
    // A crash after writing a block but before its durable checkpoint must discard its unconfirmed tail.
    let data = remote
        .path()
        .join(format!(".remote-codex-transfer-{token}/data"));
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(data)?
        .write_all(b"unconfirmed")?;
    drop(dst);
    let dst = Endpoint::open(remote.path(), "owner")?;
    assert_eq!(prepare(&dst, &token, &stamp, None)?, (CHUNK as u64, false));
    for (n, bytes) in bytes.chunks(CHUNK).enumerate().skip(1) {
        block(&dst, &token, (n * CHUNK) as u64, bytes)?;
    }
    let (reply, _) = src.dispatch(
        Command::Hash {
            path: "source".into(),
            stamp: stat(&src, "source")?,
        },
        &[],
    )?;
    let Reply::Hash { digest: hash } = reply else {
        return Err("Missing digest".into());
    };
    let commit = || Command::Commit {
        path: "target".into(),
        token: token.clone(),
        digest: hash.clone(),
    };
    dst.dispatch(commit(), &[])?;
    assert_eq!(std::fs::read(remote.path().join("target"))?, bytes);
    assert_eq!(
        std::fs::metadata(remote.path().join("target"))?
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        prepare(&dst, &token, &stamp, None)?,
        (bytes.len() as u64, true)
    );
    dst.dispatch(commit(), &[])?;
    dst.dispatch(
        Command::Cancel {
            path: "target".into(),
            token,
        },
        &[],
    )?;
    assert_eq!(std::fs::read_dir(remote.path())?.count(), 1);
    Ok(())
}
#[test]
fn lost_commit_ack_is_reconciled_without_overwriting_later_changes() -> Result {
    let root = tempfile::tempdir()?;
    let (endpoint, stamp) = source(root.path(), b"new")?;
    std::fs::write(root.path().join("target"), b"old")?;
    let old = stat(&endpoint, "target")?;
    let token = uuid::Uuid::new_v4().to_string();
    prepare(&endpoint, &token, &stamp, Some(old.clone()))?;
    block(&endpoint, &token, 0, b"new")?;
    assert_eq!(std::fs::read(root.path().join("target"))?, b"old");
    endpoint.dispatch(
        Command::Commit {
            path: "target".into(),
            token: token.clone(),
            digest: digest(b"new"),
        },
        &[],
    )?;
    let journal = root
        .path()
        .join(format!(".remote-codex-transfer-{token}/state.json"));
    let mut saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&journal)?)?;
    saved["committed"] = false.into();
    std::fs::write(&journal, serde_json::to_vec(&saved)?)?;
    assert_eq!(
        prepare(&endpoint, &token, &stamp, Some(old.clone()))?,
        (3, true)
    );
    std::fs::write(root.path().join("target"), b"later edit")?;
    assert!(prepare(&endpoint, &token, &stamp, Some(old)).is_err());
    assert_eq!(std::fs::read(root.path().join("target"))?, b"later edit");
    Ok(())
}
#[test]
fn conflicts_and_corrupt_chunks_leave_the_original_untouched() -> Result {
    let root = tempfile::tempdir()?;
    let (endpoint, stamp) = source(root.path(), b"new")?;
    std::fs::write(root.path().join("target"), b"original")?;
    let old = stat(&endpoint, "target")?;
    let token = uuid::Uuid::new_v4().to_string();
    prepare(&endpoint, &token, &stamp, Some(old))?;
    assert!(
        endpoint
            .dispatch(
                Command::Write {
                    path: "target".into(),
                    token: token.clone(),
                    offset: 0,
                    digest: digest(b"new")
                },
                b"bad"
            )
            .is_err()
    );
    block(&endpoint, &token, 0, b"new")?;
    std::fs::write(root.path().join("target"), b"edited")?;
    assert!(
        endpoint
            .dispatch(
                Command::Commit {
                    path: "target".into(),
                    token: token.clone(),
                    digest: digest(b"new")
                },
                &[]
            )
            .is_err()
    );
    endpoint.dispatch(
        Command::Cancel {
            path: "target".into(),
            token: token.clone(),
        },
        &[],
    )?;
    endpoint.dispatch(
        Command::Cancel {
            path: "target".into(),
            token,
        },
        &[],
    )?;
    assert_eq!(std::fs::read(root.path().join("target"))?, b"edited");
    assert_eq!(std::fs::read_dir(root.path())?.count(), 2);
    Ok(())
}
#[test]
fn empty_readonly_file_can_recover_after_metadata_was_applied() -> Result {
    let root = tempfile::tempdir()?;
    let (endpoint, mut stamp) = source(root.path(), b"")?;
    stamp.mode = 0o444;
    let token = uuid::Uuid::new_v4().to_string();
    prepare(&endpoint, &token, &stamp, None)?;
    let data = root
        .path()
        .join(format!(".remote-codex-transfer-{token}/data"));
    std::fs::set_permissions(data, std::fs::Permissions::from_mode(0o444))?;
    assert_eq!(prepare(&endpoint, &token, &stamp, None)?, (0, false));
    endpoint.dispatch(
        Command::Commit {
            path: "target".into(),
            token,
            digest: digest(b""),
        },
        &[],
    )?;
    assert_eq!(std::fs::metadata(root.path().join("target"))?.len(), 0);
    Ok(())
}

#[test]
fn interrupted_first_prepare_can_be_reopened_without_a_checkpoint() -> Result {
    let root = tempfile::tempdir()?;
    let (endpoint, stamp) = source(root.path(), b"new")?;
    let token = uuid::Uuid::new_v4().to_string();
    let stage = root.path().join(format!(".remote-codex-transfer-{token}"));
    std::fs::create_dir(&stage)?;
    std::fs::write(stage.join("data"), b"unacknowledged")?;
    assert_eq!(prepare(&endpoint, &token, &stamp, None)?, (0, false));
    assert_eq!(std::fs::metadata(stage.join("data"))?.len(), 0);
    block(&endpoint, &token, 0, b"new")?;
    endpoint.dispatch(
        Command::Commit {
            path: "target".into(),
            token,
            digest: digest(b"new"),
        },
        &[],
    )?;
    assert_eq!(std::fs::read(root.path().join("target"))?, b"new");
    Ok(())
}
