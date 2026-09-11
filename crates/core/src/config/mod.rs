mod environment;
mod key;
mod layer;
mod registry;
mod resolve;
mod value;

pub use environment::{EnvironmentPlan, plan_environment};
pub use key::{ConfigKey, EnvironmentName};
pub use layer::{ConfigInput, ConfigLayer};
pub use registry::{ApplyPolicy, SettingSpec, ValueKind};
pub use resolve::{EffectiveConfig, SettingView, Source, resolve};
pub use value::{ConfigValue, ProxyMode, SecretRef, VisibleValue};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("unknown configuration key")]
    UnknownKey,
    #[error("invalid environment variable name")]
    InvalidEnvironmentName,
    #[error("environment variable is reserved by remote-codex")]
    ReservedEnvironment,
    #[error("configuration key is read-only")]
    ReadOnlySetting,
    #[error("invalid configuration value: {0}")]
    InvalidValue(&'static str),
    #[error("sensitive values require a credential reference")]
    SecretRequired,
    #[error("secret values are supported only for environment settings")]
    SecretNotAllowed,
    #[error("conflicting proxy aliases in server settings")]
    ConflictingProxyAliases,
    #[error("custom proxy mode requires at least one configured proxy address")]
    MissingCustomProxy,
}

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ConfigReport {
    pub server_id: String,
    pub saved_revision: i64,
    pub applied_revision: Option<i64>,
    pub items: Vec<ConfigItem>,
}

#[derive(Debug, Serialize)]
pub struct ConfigItem {
    #[serde(flatten)]
    pub setting: SettingView,
    pub application_state: &'static str,
}
