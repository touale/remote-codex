use serde::Serialize;

use super::{ConfigKey, ConfigValue, ProxyMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyPolicy {
    Supervisor,
    NextDisconnect,
    NextReconnect,
    NewProcess,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    Boolean,
    GraceSeconds,
    RetryAttempts,
    ExecutionMode,
    Environment,
    ProxyMode,
    Identity,
}

#[derive(Clone, Debug)]
pub struct SettingSpec {
    pub kind: ValueKind,
    pub writable: bool,
    pub apply: ApplyPolicy,
    pub default: Option<ConfigValue>,
}

impl ConfigKey {
    /// The single registry for defaults, type and application policy.
    pub fn spec(&self) -> SettingSpec {
        let (kind, apply, default) = match self {
            Self::Background => (
                ValueKind::Boolean,
                ApplyPolicy::Supervisor,
                Some(ConfigValue::Boolean(true)),
            ),
            Self::DisconnectGraceSeconds => (
                ValueKind::GraceSeconds,
                ApplyPolicy::NextDisconnect,
                Some(ConfigValue::Integer(30)),
            ),
            Self::ReconnectMaxAttempts => (
                ValueKind::RetryAttempts,
                ApplyPolicy::NextReconnect,
                Some(ConfigValue::Integer(10)),
            ),
            Self::Environment(_) => (ValueKind::Environment, ApplyPolicy::NewProcess, None),
            Self::ProxyMode => (
                ValueKind::ProxyMode,
                ApplyPolicy::NewProcess,
                Some(ConfigValue::ProxyMode(ProxyMode::Direct)),
            ),
            Self::ExecutionMode => (
                ValueKind::ExecutionMode,
                ApplyPolicy::NewProcess,
                Some(ConfigValue::Text("sandboxed".into())),
            ),
            Self::SshHost | Self::SshUser | Self::SshPort => {
                (ValueKind::Identity, ApplyPolicy::ReadOnly, None)
            }
        };
        SettingSpec {
            kind,
            writable: kind != ValueKind::Identity,
            apply,
            default,
        }
    }
}

pub(crate) const DEFAULT_KEYS: [ConfigKey; 5] = [
    ConfigKey::Background,
    ConfigKey::DisconnectGraceSeconds,
    ConfigKey::ReconnectMaxAttempts,
    ConfigKey::ProxyMode,
    ConfigKey::ExecutionMode,
];
