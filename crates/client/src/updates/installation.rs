use super::{UpdateComponent, error};
use crate::Result;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

pub(super) struct Installation {
    pub component: UpdateComponent,
    pub path: PathBuf,
    pub version: String,
}

pub(super) fn current(component: UpdateComponent) -> Result<Installation> {
    let executable = std::env::current_exe()?.canonicalize()?;
    let path = executable
        .ancestors()
        .nth(3)
        .filter(|p| component == UpdateComponent::App && p.extension().is_some_and(|s| s == "app"))
        .unwrap_or(&executable)
        .to_owned();
    Ok(Installation {
        component,
        path,
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

pub(super) async fn read_version(component: UpdateComponent, path: &Path) -> Result<String> {
    let path = path.canonicalize()?;
    let invalid = || {
        error(
            "UPDATE_IDENTITY",
            "Cannot verify the Remote Codex installation.",
        )
    };
    let version = match component {
        UpdateComponent::Cli => {
            let output = tokio::time::timeout(
                Duration::from_secs(3),
                tokio::process::Command::new(&path)
                    .arg("--version")
                    .stdin(Stdio::null())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .map_err(|_| invalid())??;
            if !output.status.success() || output.stdout.len() > 1024 {
                return Err(invalid());
            }
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .strip_prefix("remote-codex ")
                .ok_or_else(invalid)?
                .to_owned()
        }
        UpdateComponent::App => {
            let value =
                plist::Value::from_file(path.join("Contents/Info.plist")).map_err(|_| invalid())?;
            let dict = value.as_dictionary().ok_or_else(invalid)?;
            if dict
                .get("CFBundleIdentifier")
                .and_then(plist::Value::as_string)
                != Some("dev.remotecodex.desktop")
            {
                return Err(invalid());
            }
            dict.get("CFBundleShortVersionString")
                .and_then(plist::Value::as_string)
                .ok_or_else(invalid)?
                .to_owned()
        }
    };
    semver::Version::parse(&version).map_err(|_| invalid())?;
    Ok(version)
}
