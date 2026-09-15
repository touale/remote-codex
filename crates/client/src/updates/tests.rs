use super::*;
use serde_json::json;
use std::fs;

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

fn fixture_release() -> serde_json::Value {
    let mut assets = Vec::new();
    for name in [
        "remote-codex-macos-arm64.tar.gz",
        "remote-codex-macos-arm64.app.tar.gz",
    ] {
        for suffix in ["", ".sig"] {
            assets.push(json!({ "name": format!("{name}{suffix}"), "size": 128,
                "browser_download_url": format!("https://github.com/touale/remote-codex/releases/download/v2.0.0/{name}{suffix}") }));
        }
    }
    json!({"tag_name":"v2.0.0", "draft":false, "prerelease":false, "assets":assets})
}

#[test]
fn release_requires_stable_versions_and_signed_assets_from_this_repository() -> TestResult {
    let mut value = fixture_release();
    let release = release::select(&value)?;
    assert!(
        release.assets[&UpdateComponent::Cli]
            .url
            .ends_with("remote-codex-macos-arm64.tar.gz")
    );
    assert!(
        release.assets[&UpdateComponent::App]
            .url
            .ends_with("remote-codex-macos-arm64.app.tar.gz")
    );
    value["assets"][1]["browser_download_url"] = json!("https://example.invalid/signature");
    assert!(
        !release::select(&value)?
            .assets
            .contains_key(&UpdateComponent::Cli)
    );
    value["draft"] = json!(true);
    assert!(release::select(&value).is_err());
    assert!(release::newer("2.0.0", "1.10.0"));
    assert!(!release::newer("1.9.0", "1.10.0"));
    assert!(!release::newer("2.0.0", "2.0.0"));
    Ok(())
}

#[test]
fn tauri_signatures_reject_changed_payloads_and_wrong_keys() -> TestResult {
    let root = tempfile::tempdir()?;
    let file = root.path().join("package");
    fs::write(&file, include_bytes!("fixtures/package.txt"))?;
    let sig = include_bytes!("fixtures/package.txt.sig");
    let key = include_str!("fixtures/test.pub");
    package::verify(&file, sig, key)?;
    assert!(package::verify(&file, sig, include_str!("public.key")).is_err());
    fs::write(&file, "changed")?;
    assert!(package::verify(&file, sig, key).is_err());
    assert!(package::verify(&file, b"not a signature", key).is_err());
    Ok(())
}

fn archive(path: &std::path::Path, name: &str, link: Option<&str>) -> TestResult {
    let zip = flate2::write::GzEncoder::new(fs::File::create(path)?, flate2::Compression::fast());
    let mut tar = tar::Builder::new(zip);
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o755);
    if let Some(link) = link {
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        tar.append_link(&mut header, name, link)?;
    } else {
        header.set_size(3);
        header.set_cksum();
        tar.append_data(&mut header, name, &b"new"[..])?;
    }
    tar.into_inner()?.finish()?;
    Ok(())
}

#[test]
fn extraction_rejects_foreign_roots_and_escaping_links() -> TestResult {
    let root = tempfile::tempdir()?;
    let file = root.path().join("archive");
    let output = root.path().join("extract");
    fs::create_dir(&output)?;
    archive(&file, "remote-codex", None)?;
    let result = package::extract(&file, &output, UpdateComponent::Cli)?;
    assert_eq!(fs::read(result)?, b"new");
    archive(&file, "other-program", None)?;
    assert!(package::extract(&file, &output, UpdateComponent::Cli).is_err());
    archive(
        &file,
        "Remote Codex.app/Contents/escape",
        Some("../../../outside"),
    )?;
    assert!(package::extract(&file, &output, UpdateComponent::App).is_err());
    assert!(!root.path().join("outside").exists());
    Ok(())
}

#[tokio::test]
async fn modes_are_shared_but_each_component_has_its_own_installation_lock() -> TestResult {
    let temporary = tempfile::tempdir()?;
    let directory = temporary.path().join("state");
    let mut updater = Updater::open(Some(directory.clone()), UpdateComponent::Cli)?;
    let app = Updater::open(Some(directory), UpdateComponent::App)?;
    assert_eq!(updater.snapshot().await?.mode, UpdateMode::Notify);
    updater.configure(UpdateMode::Manual).await?;
    assert_eq!(app.snapshot().await?.mode, UpdateMode::Manual);
    let lock = updater.lock()?;
    assert!(updater.snapshot().await?.busy);
    assert!(!app.snapshot().await?.busy);
    assert_eq!(
        updater.check().await.err().ok_or("expected busy")?.code(),
        "UPDATE_BUSY"
    );
    drop(lock);
    assert!(!updater.snapshot().await?.busy);
    updater.can_install = true;
    updater.automatic_check().await?;
    updater.can_install = false;
    assert_eq!(
        updater
            .install(&|_| {})
            .await
            .err()
            .ok_or("expected development guard")?
            .code(),
        "UPDATE_UNMANAGED"
    );
    assert!(!updater.snapshot().await?.busy);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn atomic_replacement_keeps_existing_readers_and_nonempty_bundles_alive() -> TestResult {
    use std::io::{Read, Write};
    let root = tempfile::tempdir()?;
    let installed = root.path().join("installed");
    let stage = root.path().join("stage");
    fs::write(&installed, "old")?;
    fs::write(&stage, "new")?;
    let mut reader = fs::File::open(&installed)?;
    package::replace(&stage, &installed, &fs::symlink_metadata(&installed)?)?;
    let mut old = String::new();
    reader.read_to_string(&mut old)?;
    assert_eq!(old, "old");
    assert_eq!(fs::read_to_string(&installed)?, "new");
    for (path, version) in [(&installed, "old"), (&stage, "new")] {
        fs::remove_file(path)?;
        fs::create_dir(path)?;
        fs::File::create(path.join("Contents"))?.write_all(version.as_bytes())?;
    }
    package::replace(&stage, &installed, &fs::symlink_metadata(&installed)?)?;
    assert_eq!(fs::read_to_string(installed.join("Contents"))?, "new");
    let original = fs::symlink_metadata(&installed)?;
    fs::rename(&installed, root.path().join("previous"))?;
    fs::create_dir(&installed)?;
    fs::write(installed.join("Contents"), "external update")?;
    let error = package::replace(&stage, &installed, &original)
        .err()
        .ok_or("expected conflict")?;
    assert_eq!(error.code(), "UPDATE_CHANGED");
    assert_eq!(
        fs::read_to_string(installed.join("Contents"))?,
        "external update"
    );
    Ok(())
}
