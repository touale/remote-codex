use super::State;
use crate::credentials::{self, NativeVault};
use std::sync::{Arc, atomic::Ordering};

/// Notices contain no credential values, references or raw vault diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "code")]
pub enum ClientNotice {
    #[serde(rename = "CREDENTIAL_CLEANUP_PENDING")]
    CredentialCleanupPending,
}

impl ClientNotice {
    pub fn message(&self) -> &'static str {
        match self {
            Self::CredentialCleanupPending => {
                "Old credential cleanup is pending. Unlock the system credential store if needed; remote-codex will retry on a later command."
            }
        }
    }
}

pub type NoticeHandler = Arc<dyn Fn(ClientNotice) + Send + Sync>;

impl State {
    pub(super) async fn cleanup_credentials(&self) {
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            let vault = NativeVault::new(&self.store.installation_id().await?);
            credentials::cleanup(&self.store, &vault).await
        })
        .await;
        if !matches!(result, Ok(Ok(true)))
            && !self.cleanup_warned.swap(true, Ordering::Relaxed)
            && let Some(notice) = &self.options.notice
        {
            notice(ClientNotice::CredentialCleanupPending);
        }
    }
}
