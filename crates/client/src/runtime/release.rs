use crate::{ClientError, Result, store::private_directory};
use remote_codex_protocol::Fault;
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path, time::Duration};

const ASSET: &str = "codex-package-x86_64-unknown-linux-musl.tar.gz";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Package {
    pub version: String,
    pub url: String,
    pub sha256: String,
}

impl Package {
    fn validate(&self, version: &str) -> Result<()> {
        let expected =
            format!("https://github.com/openai/codex/releases/download/rust-v{version}/{ASSET}");
        if self.version != version
            || self.url != expected
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(ClientError::Integrity);
        }
        Ok(())
    }
}

pub(super) async fn resolve(
    cache: &Path,
    version: &str,
    known: Option<&super::RuntimeInfo>,
) -> Result<Package> {
    if version.len() > 96
        || !version.starts_with(|c: char| c.is_ascii_digit())
        || !version
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c))
    {
        return Err(ClientError::Argument("invalid Codex release version"));
    }
    private_directory(cache)?;
    let path = cache.join(format!("{version}-linux-x86_64.json"));
    if path.try_exists()? {
        let package: Package = serde_json::from_slice(&std::fs::read(&path)?)?;
        package.validate(version)?;
        return Ok(package);
    }
    if let Some(known) = known.filter(|r| r.version == version && r.platform == "linux-x86_64") {
        let package = Package {
            version: version.into(),
            url: format!(
                "https://github.com/openai/codex/releases/download/rust-v{version}/{ASSET}"
            ),
            sha256: known.archive_sha256.clone(),
        };
        package.validate(version)?;
        return Ok(package);
    }
    let client = reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("remote-codex/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let response = client
        .get(format!(
            "https://api.github.com/repos/openai/codex/releases/tags/rust-v{version}"
        ))
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(Fault::new(
            "CODEX_RELEASE_UNAVAILABLE",
            &format!("no official remote execution package is published for local Codex {version}"),
        )
        .into());
    }
    let mut response = response.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
            return Err(ClientError::Integrity);
        }
        bytes.extend_from_slice(&chunk);
    }
    let package = select(&serde_json::from_slice(&bytes)?, version)?;
    let mut file = tempfile::NamedTempFile::new_in(cache)?;
    file.write_all(&serde_json::to_vec(&package)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| ClientError::Io(e.error))?;
    Ok(package)
}

fn select(release: &serde_json::Value, version: &str) -> Result<Package> {
    if release["tag_name"] != format!("rust-v{version}") || release["draft"] != false {
        return Err(ClientError::Integrity);
    }
    let asset = release["assets"]
        .as_array()
        .and_then(|assets| assets.iter().find(|a| a["name"] == ASSET))
        .ok_or_else(|| {
            Fault::new(
                "CODEX_PACKAGE_UNAVAILABLE",
                &format!("Codex {version} has no Linux x86_64 execution package"),
            )
        })?;
    let digest = asset["digest"]
        .as_str()
        .and_then(|v| v.strip_prefix("sha256:"))
        .ok_or_else(|| {
            Fault::new(
                "CODEX_PACKAGE_UNVERIFIED",
                "official package metadata has no SHA256 digest",
            )
        })?;
    let package = Package {
        version: version.into(),
        url: asset["browser_download_url"]
            .as_str()
            .ok_or(ClientError::Integrity)?
            .into(),
        sha256: digest.to_ascii_lowercase(),
    };
    package.validate(version)?;
    Ok(package)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn release_selection_requires_exact_version_asset_and_digest() -> TestResult {
        let version = "1.2.3-alpha.1";
        let mut release = json!({"tag_name":format!("rust-v{version}"),"draft":false,"assets":[{
            "name":ASSET,"digest":format!("sha256:{}","a".repeat(64)),
            "browser_download_url":format!("https://github.com/openai/codex/releases/download/rust-v{version}/{ASSET}")
        }]});
        assert_eq!(select(&release, version)?.version, version);
        assert!(select(&release, "1.2.4").is_err());
        release["assets"][0]["browser_download_url"] =
            json!("https://example.invalid/codex.tar.gz");
        assert!(select(&release, version).is_err());
        release["assets"][0]["digest"] = serde_json::Value::Null;
        assert_eq!(
            select(&release, version)
                .err()
                .ok_or("expected missing digest")?
                .code(),
            "CODEX_PACKAGE_UNVERIFIED"
        );
        release["assets"] = json!([]);
        assert_eq!(
            select(&release, version)
                .err()
                .ok_or("expected missing package")?
                .code(),
            "CODEX_PACKAGE_UNAVAILABLE"
        );
        Ok(())
    }

    #[tokio::test]
    async fn verified_installation_and_cached_metadata_work_offline() -> TestResult {
        let root = tempfile::tempdir()?;
        let cache = root.path().join("cache");
        let known = super::super::RuntimeInfo {
            version: "1.2.3".into(),
            platform: "linux-x86_64".into(),
            archive_sha256: "a".repeat(64),
        };
        let package = resolve(&cache, &known.version, Some(&known)).await?;
        let path = cache.join("1.2.3-linux-x86_64.json");
        std::fs::write(&path, serde_json::to_vec(&package)?)?;
        assert_eq!(
            resolve(&cache, "1.2.3", None).await?.sha256,
            known.archive_sha256
        );
        let invalid = Package {
            url: "https://example.invalid/archive".into(),
            ..package
        };
        std::fs::write(path, serde_json::to_vec(&invalid)?)?;
        assert!(matches!(
            resolve(&cache, "1.2.3", None).await,
            Err(ClientError::Integrity)
        ));
        Ok(())
    }
}
