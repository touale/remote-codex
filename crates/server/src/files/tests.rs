use super::*;
use remote_codex_protocol::FileRequest as F;
type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn staged_writes_preserve_conflicts_and_enforce_upload_ownership() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = root.path().to_str().ok_or("invalid test path")?;
    std::fs::write(root.path().join("file.txt"), "before")?;
    let mut files = Files::default();
    let read = files.dispatch(
        "owner",
        path,
        &F::Read {
            path: "file.txt".into(),
            offset: 0,
            revision: None,
        },
    )?;
    let revision = read["revision"]
        .as_str()
        .ok_or("missing revision")?
        .to_owned();
    let stage = files.dispatch(
        "owner",
        path,
        &F::BeginWrite {
            path: "file.txt".into(),
            revision: Some(revision),
            length: 5,
        },
    )?;
    let token = stage["token"].as_str().ok_or("missing token")?.to_owned();
    let write = F::WriteChunk {
        token: token.clone(),
        offset: 0,
        bytes: b"after".into(),
    };
    assert!(files.dispatch("other", path, &write).is_err());
    files.dispatch("owner", path, &write)?;
    std::fs::write(root.path().join("file.txt"), "external edit")?;
    assert_eq!(
        files
            .dispatch("owner", path, &F::CommitWrite { token })
            .err()
            .ok_or("expected conflict")?
            .code,
        "FILE_CONFLICT"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("file.txt"))?,
        "external edit"
    );
    assert_eq!(std::fs::read_dir(root.path())?.count(), 1);
    Ok(())
}

#[test]
fn chunked_read_and_commit_handle_utf8_and_empty_files() -> TestResult {
    let root = tempfile::tempdir()?;
    let path = root.path().to_str().ok_or("invalid path")?;
    let mut files = Files::default();
    for (name, text) in [
        ("unicode.txt", "跨服务器开发\n".repeat(24000)),
        ("empty.txt", String::new()),
    ] {
        let stage = files.dispatch(
            "owner",
            path,
            &F::BeginWrite {
                path: name.into(),
                revision: None,
                length: text.len() as u64,
            },
        )?;
        let token = stage["token"].as_str().ok_or("missing token")?.to_owned();
        for (index, bytes) in text.as_bytes().chunks(FILE_CHUNK).enumerate() {
            files.dispatch(
                "owner",
                path,
                &F::WriteChunk {
                    token: token.clone(),
                    offset: (index * FILE_CHUNK) as u64,
                    bytes: bytes.into(),
                },
            )?;
        }
        let result = files.dispatch("owner", path, &F::CommitWrite { token })?;
        let revision = result["revision"]
            .as_str()
            .ok_or("missing revision")?
            .to_owned();
        let mut bytes = Vec::new();
        loop {
            let chunk: FileChunk = serde_json::from_value(files.dispatch(
                "owner",
                path,
                &F::Read {
                    path: name.into(),
                    offset: bytes.len() as u64,
                    revision: Some(revision.clone()),
                },
            )?)?;
            bytes.extend(chunk.bytes);
            if bytes.len() as u64 == chunk.length {
                break;
            }
        }
        assert_eq!(String::from_utf8(bytes)?, text);
    }
    Ok(())
}

#[test]
fn file_capability_rejects_traversal_and_external_symlinks() -> TestResult {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("secret"), "outside")?;
    std::os::unix::fs::symlink(outside.path(), root.path().join("escape"))?;
    let path = root.path().to_str().ok_or("invalid path")?;
    let mut files = Files::default();
    for path_to_read in ["../secret", "escape/secret", "/etc/passwd"] {
        assert!(
            files
                .dispatch(
                    "owner",
                    path,
                    &F::Read {
                        path: path_to_read.into(),
                        offset: 0,
                        revision: None
                    }
                )
                .is_err()
        );
    }
    assert!(
        files
            .dispatch(
                "owner",
                path,
                &F::BeginWrite {
                    path: "escape/created".into(),
                    revision: None,
                    length: 0
                }
            )
            .is_err()
    );
    assert!(!outside.path().join("created").exists());
    assert!(
        files
            .dispatch("owner", path, &F::Remove { path: ".".into() })
            .is_err()
    );
    Ok(())
}
