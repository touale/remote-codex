use super::{UpdateComponent, error};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Serialize, Deserialize)]
pub struct UpdateRelease {
    pub version: String,
    pub url: String,
}

const REPOSITORY: &str = "https://github.com/touale/remote-codex";
const API: &str = "https://api.github.com/repos/touale/remote-codex/releases/latest";

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Asset {
    pub url: String,
    pub size: u64,
    pub signature: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Release {
    pub info: UpdateRelease,
    pub assets: BTreeMap<UpdateComponent, Asset>,
}

pub(super) fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .user_agent(concat!("remote-codex/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

pub(super) async fn small(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut response = client
        .get(url)
        .timeout(Duration::from_secs(20))
        .send()
        .await?
        .error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(error(
                "UPDATE_METADATA",
                "Update metadata exceeds its size limit.",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) async fn latest() -> Result<Release> {
    let bytes = small(&client()?, API, 2 * 1024 * 1024).await?;
    select(&serde_json::from_slice(&bytes)?)
}

pub(super) fn select(value: &serde_json::Value) -> Result<Release> {
    let invalid = || {
        error(
            "UPDATE_METADATA",
            "The latest release has invalid update metadata.",
        )
    };
    let tag = value["tag_name"].as_str().ok_or_else(invalid)?;
    let version =
        semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag)).map_err(|_| invalid())?;
    if value["draft"] != false || value["prerelease"] != false || !version.pre.is_empty() {
        return Err(invalid());
    }
    let mut assets = BTreeMap::new();
    let list = value["assets"].as_array().ok_or_else(invalid)?;
    for (component, name) in [
        (UpdateComponent::Cli, "remote-codex-cli-macos-arm64.tar.gz"),
        (UpdateComponent::App, "remote-codex-app-macos-arm64.tar.gz"),
    ] {
        let url = format!("{REPOSITORY}/releases/download/{tag}/{name}");
        let sig = format!("{url}.sig");
        let archive = list
            .iter()
            .find(|a| a["name"] == name && a["browser_download_url"] == url);
        let signature = list
            .iter()
            .find(|a| a["name"] == format!("{name}.sig") && a["browser_download_url"] == sig);
        if let (Some(archive), Some(_)) = (archive, signature) {
            let size = archive["size"]
                .as_u64()
                .filter(|s| *s > 0 && *s <= 1024 * 1024 * 1024)
                .ok_or_else(invalid)?;
            assets.insert(
                component,
                Asset {
                    url,
                    size,
                    signature: sig,
                },
            );
        }
    }
    Ok(Release {
        info: UpdateRelease {
            version: version.to_string(),
            url: format!("{REPOSITORY}/releases/tag/{tag}"),
        },
        assets,
    })
}

pub(super) fn newer(new: &str, old: &str) -> bool {
    matches!((semver::Version::parse(new), semver::Version::parse(old)), (Ok(new), Ok(old)) if new > old)
}
