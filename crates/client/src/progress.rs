use serde::Serialize;

/// Preparation events contain data only; terminal and desktop clients own rendering.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum PrepareEvent {
    Stage(PrepareStage),
    Transfer(TransferProgress),
    ServiceWaiting(remote_codex_protocol::ServiceActivity),
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
    WaitService,
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
