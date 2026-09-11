use serde::Serialize;
use std::{future::Future, sync::Arc};
pub type ProgressHandler = Arc<dyn Fn(PrepareEvent) + Send + Sync>;
tokio::task_local! { static OPERATION: ProgressHandler; }
pub async fn with_progress<F: Future>(handler: ProgressHandler, future: F) -> F::Output {
    OPERATION.scope(handler, future).await
}
pub(crate) fn current() -> Option<ProgressHandler> {
    OPERATION.try_with(Clone::clone).ok()
}

/// Preparation events contain data only; terminal and desktop clients own rendering.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum PrepareEvent {
    Stage(PrepareStage),
    Transfer(TransferProgress),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrepareStage {
    ConnectSsh,
    InspectHost,
    InspectRuntime,
    InspectCache,
    UseCachedPackage,
    Download,
    VerifyDownload,
    Upload,
    VerifyInstall,
    Prepared,
    InstallService,
    StartService,
    UseRunningService,
    Synchronize,
    StartLocalCodex,
    ConnectExecution,
    PrepareSkills,
    OpenLocalSession,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    Download,
    Upload,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct TransferProgress {
    pub kind: TransferKind,
    /// Download bytes written to the local file, or upload bytes handed to SSH.
    /// This does not acknowledge remote receipt, checksum verification or installation.
    pub transferred_bytes: u64,
    /// None when the HTTP response does not provide a content length.
    pub total_bytes: Option<u64>,
}
