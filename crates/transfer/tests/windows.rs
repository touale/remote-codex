#![cfg(windows)]
use remote_codex_protocol::transfer::{Command, Reply};
use remote_codex_transfer::Endpoint;

type Result = std::result::Result<(), Box<dyn std::error::Error>>;
#[test]
fn windows_names_and_case_collisions_are_rejected() -> Result {
    let root = tempfile::tempdir()?;
    std::fs::write(root.path().join("Report.txt"), "original")?;
    let endpoint = Endpoint::open(root.path(), "test")?;
    for path in [
        "../outside",
        "C:/outside",
        "file:stream",
        "NUL.txt",
        "trailing.",
        "dir\\file",
    ] {
        assert!(
            endpoint
                .dispatch(Command::Stat { path: path.into() }, &[])
                .is_err(),
            "accepted {path}"
        );
    }
    let (reply, _) = endpoint.dispatch(
        Command::Stat {
            path: "Report.txt".into(),
        },
        &[],
    )?;
    let Reply::Stat {
        stamp: Some(source),
    } = reply
    else {
        return Err("missing source".into());
    };
    let token = uuid::Uuid::new_v4().to_string();
    let prepared = endpoint.dispatch(
        Command::Prepare {
            path: "report.txt".into(),
            token,
            source,
            expected: None,
        },
        &[],
    );
    assert!(prepared.is_err());
    assert_eq!(
        std::fs::read_to_string(root.path().join("Report.txt"))?,
        "original"
    );
    Ok(())
}
#[test]
fn junctions_cannot_escape_the_transfer_root() -> Result {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("secret"), "private")?;
    let status = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(root.path().join("link"))
        .arg(outside.path())
        .output()?;
    assert!(status.status.success());
    let endpoint = Endpoint::open(root.path(), "test")?;
    assert!(
        endpoint
            .dispatch(
                Command::Stat {
                    path: "link/secret".into()
                },
                &[]
            )
            .is_err()
    );
    std::fs::remove_dir(root.path().join("link"))?;
    Ok(())
}
