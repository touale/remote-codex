pub type Result<T> = std::result::Result<T, ClientError>;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("{0}")]
    Argument(&'static str),
    #[error("session is already open in another frontend; use --takeover to transfer control")]
    SessionInUse,
    #[error(
        "local and project MCP names conflict; select --mcp-source local or --mcp-source remote"
    )]
    McpNameConflict,
    #[error(transparent)]
    Config(#[from] remote_codex_core::config::ConfigError),
    #[error(transparent)]
    Endpoint(#[from] remote_codex_core::connection::EndpointError),
    #[error("local storage operation failed")]
    Storage(#[from] sqlx::Error),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid stored data")]
    Data(#[from] serde_json::Error),
    #[error("unsupported database format; this release requires local schema 13")]
    Schema,
    #[error("state directory and database must be private, regular paths")]
    PrivatePath,
    #[error("connection was not found")]
    NotFound,
    #[error("connection name refers to another SSH endpoint")]
    NameConflict,
    #[error("configuration changed; read the new revision and retry")]
    RevisionConflict,
    #[error("this capability is not implemented: {0}")]
    Unsupported(&'static str),
    #[error("SSH operation failed (exit {0}); the saved connection is available for retry")]
    Ssh(i32),
    #[error("operation timed out; remote outcome must be inspected before retrying")]
    Timeout,
    #[error("invalid remote response")]
    RemoteResponse,
    #[error("runtime package verification failed")]
    Integrity,
    #[error("download failed; check network settings")]
    Download(#[from] reqwest::Error),
    #[error("local credential store is unavailable; unlock it and retry")]
    Credentials,
    #[error("{0}: {1}")]
    RemoteFault(String, String, bool),
    #[error("Codex frontend exited with {0}")]
    CodexExit(u8),
}

impl ClientError {
    pub fn code(&self) -> &str {
        match self {
            Self::SessionInUse => "SESSION_IN_USE",
            Self::McpNameConflict => "MCP_NAME_CONFLICT",
            Self::Argument(_) => "INVALID_ARGUMENT",
            Self::Config(_) => "INVALID_CONFIGURATION",
            Self::Endpoint(_) => "INVALID_SSH_TARGET",
            Self::Storage(_) => "STORAGE_ERROR",
            Self::Io(_) => "IO_ERROR",
            Self::Data(_) => "INVALID_STORED_DATA",
            Self::Schema => "UNSUPPORTED_SCHEMA",
            Self::PrivatePath => "INSECURE_STATE_PATH",
            Self::NotFound => "NOT_FOUND",
            Self::NameConflict => "CONNECTION_NAME_CONFLICT",
            Self::RevisionConflict => "REVISION_CONFLICT",
            Self::Unsupported(_) => "UNSUPPORTED_CAPABILITY",
            Self::Ssh(255) => "SSH_FAILED",
            Self::Ssh(75) => "REMOTE_BUSY",
            Self::Ssh(_) => "REMOTE_OPERATION_FAILED",
            Self::Timeout => "OPERATION_TIMEOUT",
            Self::RemoteResponse => "INVALID_REMOTE_RESPONSE",
            Self::Integrity => "INTEGRITY_MISMATCH",
            Self::Download(_) => "DOWNLOAD_FAILED",
            Self::Credentials => "CREDENTIAL_BACKEND_UNAVAILABLE",
            Self::RemoteFault(code, _, _) => code.as_str(),
            Self::CodexExit(_) => "CODEX_EXITED",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(
            self,
            Self::RevisionConflict
                | Self::Ssh(255)
                | Self::Ssh(75)
                | Self::Timeout
                | Self::Download(_)
        ) || matches!(self, Self::RemoteFault(code, _, false) if matches!(code.as_str(), "SERVICE_UPDATE_BUSY" | "SETTINGS_BUSY"))
    }

    pub fn outcome_is_unknown(&self) -> bool {
        if let Self::RemoteFault(_, _, unknown) = self {
            return *unknown;
        }
        matches!(
            self,
            Self::Storage(_) | Self::Io(_) | Self::Ssh(_) | Self::Timeout | Self::RemoteResponse
        )
    }

    pub fn exit_code(&self) -> u8 {
        match self {
            Self::CodexExit(code) => *code,
            Self::NameConflict | Self::RevisionConflict => 6,
            Self::Ssh(255) | Self::Timeout | Self::Download(_) => 3,
            Self::Ssh(75) => 6,
            Self::RemoteFault(code, _, _) if code == "SERVICE_UPDATE_BUSY" => 6,
            Self::Unsupported(_) | Self::Integrity | Self::Schema => 5,
            Self::Config(_)
            | Self::Endpoint(_)
            | Self::NotFound
            | Self::Argument(_)
            | Self::McpNameConflict => 2,
            _ => 7,
        }
    }
}

impl From<remote_codex_protocol::Fault> for ClientError {
    fn from(value: remote_codex_protocol::Fault) -> Self {
        Self::RemoteFault(value.code, value.message, value.outcome_unknown)
    }
}
