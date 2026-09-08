use std::collections::BTreeMap;

use super::value::{parse_value, requires_secret};
use super::{ConfigError, ConfigKey, ConfigValue, SecretRef};

/// The credential adapter stores raw secrets before constructing this input.
/// Raw text is borrowed only for validation and never retained by the layer.
pub enum ConfigInput<'a> {
    Plain(&'a str),
    Secret { reference: SecretRef, raw: &'a str },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredSetting {
    pub spelling: String,
    pub value: ConfigValue,
}

/// Validated settings explicitly saved for one server, without built-in defaults.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigLayer {
    pub(crate) settings: BTreeMap<ConfigKey, StoredSetting>,
}

impl ConfigLayer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &ConfigKey) -> Option<&ConfigValue> {
        self.settings.get(key).map(|entry| &entry.value)
    }

    pub fn entries(&self) -> impl Iterator<Item = (&ConfigKey, &str, &ConfigValue)> {
        self.settings
            .iter()
            .map(|(key, entry)| (key, entry.spelling.as_str(), &entry.value))
    }

    /// Restore an already validated credential reference from versioned storage.
    /// The credential adapter must validate the resolved value before execution.
    pub fn restore_secret(
        &mut self,
        spelling: &str,
        reference: SecretRef,
    ) -> Result<(), ConfigError> {
        let key = self.writable_key(spelling)?;
        if !matches!(key, ConfigKey::Environment(_)) {
            return Err(ConfigError::SecretNotAllowed);
        }
        if self.settings.contains_key(&key) {
            return Err(ConfigError::ConflictingProxyAliases);
        }
        self.settings.insert(
            key,
            StoredSetting {
                spelling: spelling.to_owned(),
                value: ConfigValue::Secret(reference),
            },
        );
        Ok(())
    }

    pub fn set(&mut self, key: &str, input: ConfigInput<'_>) -> Result<(), ConfigError> {
        let parsed = self.writable_key(key)?;
        let (raw, reference) = match input {
            ConfigInput::Plain(raw) => (raw, None),
            ConfigInput::Secret { reference, raw } => (raw, Some(reference)),
        };
        let value = parse_value(&parsed, raw)?;
        if reference.is_some() && !matches!(parsed, ConfigKey::Environment(_)) {
            return Err(ConfigError::SecretNotAllowed);
        }
        if reference.is_none() && requires_secret(&parsed, raw) {
            return Err(ConfigError::SecretRequired);
        }
        let value = reference.map(ConfigValue::Secret).unwrap_or(value);
        if let ConfigKey::Environment(name) = &parsed
            && name.is_proxy()
            && let Some(previous) = self.settings.get(&parsed)
            && previous.spelling != key
            && previous.value != value
        {
            return Err(ConfigError::ConflictingProxyAliases);
        }
        // Preserve the original spelling so repeating an equal alias cannot
        // silently unlock a later conflicting update using the other spelling.
        let spelling = self
            .settings
            .get(&parsed)
            .map_or_else(|| key.to_owned(), |entry| entry.spelling.clone());
        self.settings
            .insert(parsed, StoredSetting { spelling, value });
        Ok(())
    }

    pub fn unset(&mut self, key: &str) -> Result<bool, ConfigError> {
        let parsed = self.writable_key(key)?;
        Ok(self.settings.remove(&parsed).is_some())
    }

    fn writable_key(&self, key: &str) -> Result<ConfigKey, ConfigError> {
        let parsed = ConfigKey::parse(key)?;
        let spec = parsed.spec();
        if !spec.writable {
            return Err(ConfigError::ReadOnlySetting);
        }
        Ok(parsed)
    }
}
