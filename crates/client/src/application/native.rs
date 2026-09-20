use super::{AccountStatus, Client};
use crate::Result;
use remote_codex_adapter::desktop::NativeAccount;
use std::path::PathBuf;

pub struct NativeService {
    pub(super) client: Client,
}
pub struct NativeHandle {
    account: NativeAccount,
    program: PathBuf,
    version: String,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct NativeStatus {
    pub program: PathBuf,
    pub version: String,
    pub account: AccountStatus,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct LoginStart {
    pub id: String,
    pub url: String,
}
impl NativeService {
    pub async fn defaults(
        &self,
        native: &NativeHandle,
        server: &str,
    ) -> Result<super::SessionDefaults> {
        let record = self.client.0.store.find_connection(server).await?;
        let snapshot = self.client.0.store.config_snapshot(&record.id).await?;
        Ok(native
            .account
            .defaults(crate::local::execution_mode(&snapshot.effective)?)
            .await?)
    }
    pub async fn open(&self) -> Result<NativeHandle> {
        let program = self.client.0.program().await?;
        let inspected = remote_codex_adapter::program::inspect(&program).await?;
        let mut launch = program.clone();
        launch.path.clone_from(&inspected.path);
        let account = NativeAccount::open(&launch, &crate::local::codex_home()?).await?;
        if let Err(error) = inspected.verify() {
            account.close().await;
            return Err(error.into());
        }
        Ok(NativeHandle {
            account,
            program: program.path,
            version: inspected.version,
        })
    }
}
impl NativeHandle {
    pub async fn status(&self) -> Result<NativeStatus> {
        Ok(NativeStatus {
            program: self.program.clone(),
            version: self.version.clone(),
            account: self.account.status().await?,
        })
    }
    pub async fn login(&self) -> Result<LoginStart> {
        let (id, url) = self.account.login().await?;
        Ok(LoginStart { id, url })
    }
    pub async fn cancel_login(&self, id: &str) -> Result<()> {
        Ok(self.account.cancel(id).await?)
    }
    pub async fn usage(&self) -> Result<remote_codex_core::status::AccountUsage> {
        Ok(self.account.usage().await?)
    }
    pub async fn close(&self) {
        self.account.close().await;
    }
}
