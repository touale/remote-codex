use super::*;

const ID: &str = "11111111-2222-4333-8444-555555555555";

#[test]
fn composer_paths_and_bounded_utf8_reads() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let directory = home.path().join("attachments").join(ID);
    let path = directory.join("pasted-text-1.txt");
    let mut create = json!({"path":directory,"recursive":true});
    prepare("fs/createDirectory", &mut create, home.path())?;
    std::fs::create_dir_all(&directory)?;
    let text = "长目标，保留内容与换行。\nSecond line.";
    std::fs::write(&path, text)?;
    assert_eq!(
        read_text(home.path(), path.to_str().ok_or("path")?, 1024)?,
        text
    );
    let error = read_text(home.path(), path.to_str().ok_or("path")?, 10)
        .err()
        .ok_or("limit was ignored")?;
    assert_eq!(error.code, "ATTACHMENT_LIMIT");
    assert!(
        prepare(
            "fs/writeFile",
            &mut json!({"path":path,"dataBase64":"YQ==","environmentId":"remote"}),
            home.path()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn host_paths_cannot_escape_attachment_directories() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    for path in [
        home.path().join("auth.json"),
        home.path().join("attachments/../auth.json"),
        home.path()
            .join(format!("attachments/{ID}/../../auth.json")),
        home.path().join("attachments/not-a-uuid/text.txt"),
        home.path().join(format!("attachments/{ID}")),
    ] {
        assert!(prepare("fs/readFile", &mut json!({"path":path}), home.path()).is_err());
    }
    let outside = tempfile::tempdir()?;
    std::fs::write(outside.path().join("secret"), "private")?;
    let directory = home.path().join("attachments").join(ID);
    std::fs::create_dir_all(&directory)?;
    for target in [
        outside.path().join("secret"),
        outside.path().join("missing"),
    ] {
        let link = directory.join("linked.txt");
        std::os::unix::fs::symlink(target, &link)?;
        assert!(read_text(home.path(), link.to_str().ok_or("path")?, 1024).is_err());
        assert!(
            prepare(
                "fs/writeFile",
                &mut json!({"path":link,"dataBase64":"YQ=="}),
                home.path()
            )
            .is_err()
        );
        std::fs::remove_file(link)?;
    }
    std::fs::remove_dir_all(home.path().join("attachments"))?;
    std::os::unix::fs::symlink(outside.path(), home.path().join("attachments"))?;
    assert!(
        prepare(
            "fs/createDirectory",
            &mut json!({"path":directory}),
            home.path()
        )
        .is_err()
    );
    Ok(())
}
