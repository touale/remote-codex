use std::collections::{BTreeMap, BTreeSet};

use super::key::PROXY_NAMES;
use super::value::parse_value;
use super::{ConfigError, ConfigKey, ConfigValue, EffectiveConfig, EnvironmentName, ProxyMode};

/// A process adapter clears `remove`, resolves secret references, then applies
/// `set`. This does not mutate the server or the current process environment.
#[derive(Clone, Debug)]
pub struct EnvironmentPlan {
    pub remove: BTreeSet<String>,
    pub set: BTreeMap<String, ConfigValue>,
}

pub fn plan_environment(
    config: &EffectiveConfig,
    remote_environment: &BTreeMap<String, String>,
) -> Result<EnvironmentPlan, ConfigError> {
    let mode = config.proxy_mode();
    let mut plan = EnvironmentPlan {
        remove: BTreeSet::new(),
        set: BTreeMap::new(),
    };
    for name in PROXY_NAMES {
        plan.remove.insert(name.to_owned());
        plan.remove.insert(name.to_ascii_uppercase());
    }
    // Also remove any unusual casing that was present in the remote snapshot.
    for name in remote_environment.keys() {
        if PROXY_NAMES.contains(&name.to_ascii_lowercase().as_str()) {
            plan.remove.insert(name.clone());
        }
    }
    let mut proxies = BTreeMap::<EnvironmentName, ConfigValue>::new();
    if mode == ProxyMode::Inherit {
        for (raw_name, raw_value) in remote_environment {
            if !PROXY_NAMES.contains(&raw_name.to_ascii_lowercase().as_str()) {
                continue;
            }
            let name = EnvironmentName::parse(raw_name)?;
            let key = ConfigKey::Environment(name.clone());
            // Configured values replace both inherited aliases as one setting.
            if config.get(&key).is_some() {
                continue;
            }
            // Empty inherited proxy addresses mean disabled, not a malformed URL.
            let value = if raw_value.is_empty() {
                ConfigValue::Text(String::new())
            } else {
                parse_value(&key, raw_value)?
            };
            if let Some(previous) = proxies.insert(name, value.clone())
                && previous != value
            {
                return Err(ConfigError::ConflictingProxyAliases);
            }
        }
    }
    for (key, setting) in &config.settings {
        if let ConfigKey::Environment(name) = key {
            if name.is_proxy() {
                if mode != ProxyMode::Direct {
                    proxies.insert(name.clone(), setting.value.clone());
                }
            } else {
                plan.set
                    .insert(name.as_str().to_owned(), setting.value.clone());
            }
        }
    }
    for (name, value) in proxies {
        plan.set.insert(name.as_str().to_owned(), value.clone());
        plan.set.insert(name.as_str().to_ascii_uppercase(), value);
    }
    Ok(plan)
}
