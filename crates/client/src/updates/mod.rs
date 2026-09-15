mod installation;
mod package;
mod release;
mod storage;
#[cfg(test)]
mod tests;

use crate::{ClientError, Result};
pub use package::UpdateProgress;
pub use release::UpdateRelease;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

pub const UPDATE_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateMode {
    #[default]
    Notify,
    Auto,
    Manual,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum UpdateComponent {
    Cli,
    App,
}

#[derive(Serialize)]
pub struct UpdateSnapshot {
    pub mode: UpdateMode,
    pub last_checked: Option<u64>,
    pub latest: Option<UpdateRelease>,
    pub current_version: String,
    pub installed_version: String,
    pub path: PathBuf,
    pub can_install: bool,
    pub busy: bool,
    pub available: bool,
    pub restart_required: bool,
}

pub struct Updater {
    root: PathBuf,
    current: installation::Installation,
    can_install: bool,
}

fn error(code: &str, message: &str) -> ClientError {
    ClientError::RemoteFault(code.into(), message.into(), false)
}

impl Updater {
    pub fn open(directory: Option<PathBuf>, component: UpdateComponent) -> Result<Self> {
        let root = directory
            .map(Ok)
            .unwrap_or_else(crate::store::default_data_dir)?
            .join("updates");
        crate::store::private_directory(&root)?;
        Ok(Self {
            root,
            current: installation::current(component)?,
            can_install: option_env!("REMOTE_CODEX_RELEASE") == Some("1")
                && cfg!(all(target_os = "macos", target_arch = "aarch64")),
        })
    }

    pub async fn snapshot(&self) -> Result<UpdateSnapshot> {
        let saved = storage::read(&self.root)?;
        let installed_version = if self.can_install {
            installation::read_version(self.current.component, &self.current.path).await?
        } else {
            self.current.version.clone()
        };
        Ok(UpdateSnapshot {
            available: saved
                .release
                .as_ref()
                .is_some_and(|r| release::newer(&r.info.version, &installed_version)),
            restart_required: release::newer(&installed_version, &self.current.version),
            mode: saved.mode,
            last_checked: saved.last_checked,
            latest: saved.release.map(|r| r.info),
            current_version: self.current.version.clone(),
            installed_version,
            path: self.current.path.clone(),
            can_install: self.can_install,
            busy: storage::lock(&self.root, self.lock_name())?.is_none(),
        })
    }

    pub async fn configure(&self, mode: UpdateMode) -> Result<UpdateSnapshot> {
        storage::change(&self.root, |s| s.mode = mode)?;
        self.snapshot().await
    }

    pub async fn check(&self) -> Result<UpdateSnapshot> {
        let lock = self.lock()?;
        self.refresh_release().await?;
        drop(lock);
        self.snapshot().await
    }

    pub async fn install(
        &self,
        report: &(dyn Fn(UpdateProgress) + Send + Sync),
    ) -> Result<UpdateSnapshot> {
        if !self.can_install {
            return Err(error(
                "UPDATE_UNMANAGED",
                "Install a GitHub Release to enable updates for this development build.",
            ));
        }
        let lock = self.lock()?;
        let release = self.refresh_release().await?;
        package::install(&self.root, &self.current, &release, report).await?;
        drop(lock);
        self.snapshot().await
    }

    pub async fn automatic_check(&self) -> Result<()> {
        let saved = storage::read(&self.root)?;
        if !self.can_install || saved.mode == UpdateMode::Manual {
            return Ok(());
        }
        let Some(lock) = storage::lock(&self.root, self.lock_name())? else {
            return Ok(());
        };
        let release = match saved.release.filter(|_| {
            saved
                .last_checked
                .is_some_and(|t| storage::now().saturating_sub(t) < UPDATE_CHECK_INTERVAL.as_secs())
        }) {
            Some(release) => release,
            None => self.refresh_release().await?,
        };
        if storage::read(&self.root)?.mode == UpdateMode::Auto {
            package::install(&self.root, &self.current, &release, &|_| {}).await?;
        }
        drop(lock);
        Ok(())
    }

    fn lock_name(&self) -> &'static str {
        match self.current.component {
            UpdateComponent::Cli => "cli.lock",
            UpdateComponent::App => "app.lock",
        }
    }

    fn lock(&self) -> Result<nix::fcntl::Flock<std::fs::File>> {
        storage::lock(&self.root, self.lock_name())?
            .ok_or_else(|| error("UPDATE_BUSY", "Another update is already running."))
    }

    async fn refresh_release(&self) -> Result<release::Release> {
        let release = release::latest().await?;
        storage::change(&self.root, |s| {
            s.last_checked = Some(storage::now());
            s.release = Some(release.clone());
        })?;
        Ok(release)
    }
}
