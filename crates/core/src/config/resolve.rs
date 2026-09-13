use std::collections::BTreeMap;

use serde::Serialize;

use super::registry::DEFAULT_KEYS;
use super::{
    ApplyPolicy, ConfigError, ConfigKey, ConfigLayer, ConfigValue, ProxyMode, VisibleValue,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Default,
    Server,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EffectiveSetting {
    pub value: ConfigValue,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveConfig {
    pub(crate) settings: BTreeMap<ConfigKey, EffectiveSetting>,
}

/// Safe for both human and JSON output. Runtime application status belongs to
/// the service acknowledgement; resolving a value does not imply it is applied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SettingView {
    pub key: String,
    pub value: Option<VisibleValue>,
    pub redacted: bool,
    pub source: Source,
    pub apply_policy: ApplyPolicy,
    pub masked_by_proxy_mode: bool,
}

pub fn resolve(server: &ConfigLayer) -> Result<EffectiveConfig, ConfigError> {
    let mut settings = BTreeMap::new();
    for key in DEFAULT_KEYS {
        if let Some(value) = key.spec().default {
            settings.insert(
                key,
                EffectiveSetting {
                    value,
                    source: Source::Default,
                },
            );
        }
    }
    for (key, setting) in &server.settings {
        settings.insert(
            key.clone(),
            EffectiveSetting {
                value: setting.value.clone(),
                source: Source::Server,
            },
        );
    }
    let config = EffectiveConfig { settings };
    if config.proxy_mode() == ProxyMode::Custom
        && !config
            .settings
            .keys()
            .any(|key| matches!(key, ConfigKey::Environment(name) if name.is_proxy_address()))
    {
        return Err(ConfigError::MissingCustomProxy);
    }
    Ok(config)
}

impl EffectiveConfig {
    /// Internal execution adapters may need credential references. UI adapters
    /// should use `view`/`list`, which never expose secret handles or values.
    pub fn get(&self, key: &ConfigKey) -> Option<&ConfigValue> {
        self.settings.get(key).map(|entry| &entry.value)
    }

    /// Typed values for execution and configuration comparison; UI uses `list`.
    pub fn entries(&self) -> impl Iterator<Item = (&ConfigKey, &ConfigValue)> {
        self.settings.iter().map(|(key, entry)| (key, &entry.value))
    }

    pub fn proxy_mode(&self) -> ProxyMode {
        match self.get(&ConfigKey::ProxyMode) {
            Some(ConfigValue::ProxyMode(mode)) => *mode,
            _ => ProxyMode::Direct,
        }
    }

    pub fn view(&self, key: &ConfigKey) -> Option<SettingView> {
        self.settings
            .get(key)
            .map(|setting| self.setting_view(key, setting))
    }

    pub fn list(&self) -> Vec<SettingView> {
        self.settings
            .iter()
            .map(|(key, entry)| self.setting_view(key, entry))
            .collect()
    }

    fn setting_view(&self, key: &ConfigKey, setting: &EffectiveSetting) -> SettingView {
        SettingView {
            key: key.name(),
            value: setting.value.visible(),
            redacted: matches!(setting.value, ConfigValue::Secret(_)),
            source: setting.source,
            apply_policy: key.spec().apply,
            masked_by_proxy_mode: self.proxy_mode() == ProxyMode::Direct
                && matches!(key, ConfigKey::Environment(name) if name.is_proxy()),
        }
    }
}
