use remote_codex_protocol::transfer::{Command, Reply};
use remote_codex_transfer::Endpoint;
type Result = std::result::Result<(), Box<dyn std::error::Error>>;
#[test]
fn traversal_symlinks_and_other_owners_cannot_access_transfer_files() -> Result {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("secret"), b"private")?;
    std::os::unix::fs::symlink(outside.path(), root.path().join("link"))?;
    let endpoint = Endpoint::open(root.path(), "one")?;
    for path in ["../secret", "/secret", "link/secret"] {
        assert!(
            endpoint
                .dispatch(Command::Stat { path: path.into() }, &[])
                .is_err()
        );
    }
    std::fs::write(root.path().join("source"), b"source")?;
    let (reply, _) = endpoint.dispatch(
        Command::Stat {
            path: "source".into(),
        },
        &[],
    )?;
    let Reply::Stat { stamp: Some(stamp) } = reply else {
        return Err("missing source".into());
    };
    let token = uuid::Uuid::new_v4().to_string();
    endpoint.dispatch(
        Command::Prepare {
            path: "target".into(),
            token: token.clone(),
            source: stamp,
            expected: None,
        },
        &[],
    )?;
    let other = Endpoint::open(root.path(), "two")?;
    assert!(
        other
            .dispatch(
                Command::Cancel {
                    path: "target".into(),
                    token
                },
                &[]
            )
            .is_err()
    );
    assert_eq!(std::fs::read(outside.path().join("secret"))?, b"private");
    Ok(())
}
#[test]
fn paginated_directory_walk_includes_hidden_files_and_empty_folders() -> Result {
    let root = tempfile::tempdir()?;
    std::fs::create_dir(root.path().join("empty"))?;
    std::fs::write(root.path().join(".hidden"), b"")?;
    for n in 0..207 {
        std::fs::write(root.path().join(format!("{n:03}")), b"")?;
    }
    let endpoint = Endpoint::open(root.path(), "owner")?;
    let mut after = None;
    let mut names = Vec::new();
    loop {
        let (reply, _) = endpoint.dispatch(
            Command::List {
                path: "".into(),
                after,
            },
            &[],
        )?;
        let Reply::List(page) = reply else {
            return Err("missing page".into());
        };
        assert!(page.entries.len() <= 100);
        names.extend(page.entries.into_iter().map(|e| e.name));
        after = page.after;
        if after.is_none() {
            break;
        }
    }
    assert_eq!(names.len(), 209);
    assert!(names.contains(&".hidden".into()));
    assert!(names.contains(&"empty".into()));
    let unique: std::collections::BTreeSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len());
    Ok(())
}
