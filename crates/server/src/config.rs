use crate::{Checked, Result};
use remote_codex_core::config::{
    ConfigInput, ConfigKey, ConfigLayer, ConfigValue, SecretRef, plan_environment, resolve,
};
use remote_codex_protocol::{Fault, RemoteConfig};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub(crate) struct Runtime {
    pub(crate) config: RemoteConfig,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) background: bool,
    pub(crate) grace: u64,
    pub(crate) unrestricted: bool,
    pub(crate) mcp: Vec<remote_codex_protocol::ExecutionCommand>,
}

impl Runtime {
    pub(crate) async fn load(config: RemoteConfig) -> Result<Self> {
        let program = Path::new(&config.codex);
        if !program.is_absolute() || !program.is_file() {
            return Err(Fault::new(
                "INVALID_RUNTIME",
                "managed Codex executable is missing",
            ));
        }
        let mut layer = ConfigLayer::new();
        let mut secrets = BTreeMap::new();
        for (key, value) in &config.values {
            if key.starts_with("env.") {
                let reference = SecretRef::new(&uuid::Uuid::new_v4().to_string())
                    .checked("INVALID_CONFIG", "invalid environment identity")?;
                secrets.insert(reference.id().to_owned(), value.clone());
                layer
                    .set(
                        key,
                        ConfigInput::Secret {
                            reference,
                            raw: value,
                        },
                    )
                    .checked("INVALID_CONFIG", "invalid remote environment setting")?;
            } else {
                layer
                    .set(key, ConfigInput::Plain(value))
                    .checked("INVALID_CONFIG", "invalid remote configuration")?;
            }
        }
        let config_core =
            resolve(&layer).checked("INVALID_CONFIG", "invalid effective configuration")?;
        let inherited = std::env::vars().collect();
        let plan = plan_environment(&config_core, &inherited).checked(
            "INVALID_PROXY",
            "invalid or conflicting remote proxy settings",
        )?;
        let mut env = BTreeMap::new();
        for (key, value) in plan.set {
            let value = match value {
                ConfigValue::Text(text) => text,
                ConfigValue::Secret(reference) => secrets
                    .get(reference.id())
                    .cloned()
                    .ok_or_else(|| Fault::new("INVALID_CONFIG", "unresolved remote credential"))?,
                _ => {
                    return Err(Fault::new(
                        "INVALID_CONFIG",
                        "invalid process environment value",
                    ));
                }
            };
            env.insert(key, value);
        }
        let background = matches!(
            config_core.get(&ConfigKey::Background),
            Some(ConfigValue::Boolean(true))
        );
        let grace = match config_core.get(&ConfigKey::DisconnectGraceSeconds) {
            Some(ConfigValue::Integer(n)) => u64::from(*n),
            _ => 30,
        };
        let unrestricted = matches!(config_core.get(&ConfigKey::ExecutionMode),Some(ConfigValue::Text(mode)) if mode=="unrestricted");
        Ok(Self {
            config,
            env,
            background,
            grace,
            unrestricted,
            mcp: Vec::new(),
        })
    }
}

pub(crate) async fn canonical_directory(path: &str) -> Result<String> {
    if !Path::new(path).is_absolute() || path.chars().any(char::is_control) {
        return Err(Fault::new(
            "INVALID_WORKSPACE",
            "workspace must be a remote absolute directory",
        ));
    }
    let canonical: PathBuf = tokio::fs::canonicalize(path).await.checked(
        "WORKSPACE_UNAVAILABLE",
        "remote directory does not exist or is not accessible",
    )?;
    if !canonical.is_dir() {
        return Err(Fault::new(
            "INVALID_WORKSPACE",
            "remote workspace is not a directory",
        ));
    }
    let _ = tokio::fs::read_dir(&canonical)
        .await
        .checked("WORKSPACE_UNAVAILABLE", "remote directory is not readable")?;
    canonical
        .into_os_string()
        .into_string()
        .checked("INVALID_WORKSPACE", "workspace is not valid UTF-8")
}
