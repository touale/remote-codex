use std::fmt;

use serde::Serialize;
use url::Url;

use super::{ConfigError, ConfigKey, ValueKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    Inherit,
    Custom,
    Direct,
}

/// An opaque credential-store handle, never the secret itself.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretRef(String);

impl SecretRef {
    pub fn new(id: &str) -> Result<Self, ConfigError> {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
        {
            return Err(ConfigError::InvalidValue("invalid credential reference"));
        }
        Ok(Self(id.to_owned()))
    }

    /// Intended for credential adapters, not ordinary configuration output.
    pub fn id(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretRef([redacted])")
    }
}

/// Deliberately not Serialize: public views must go through redaction.
#[derive(Clone, PartialEq, Eq)]
pub enum ConfigValue {
    Boolean(bool),
    Integer(u16),
    Text(String),
    ProxyMode(ProxyMode),
    Secret(SecretRef),
}

impl fmt::Debug for ConfigValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(_) => f.write_str("Text([redacted])"),
            Self::Secret(_) => f.write_str("Secret([redacted])"),
            _ => self.visible().fmt(f),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum VisibleValue {
    Boolean(bool),
    Integer(u16),
    Text(String),
    ProxyMode(ProxyMode),
}

impl ConfigValue {
    pub fn visible(&self) -> Option<VisibleValue> {
        match self {
            Self::Boolean(value) => Some(VisibleValue::Boolean(*value)),
            Self::Integer(value) => Some(VisibleValue::Integer(*value)),
            Self::Text(value) => Some(VisibleValue::Text(value.clone())),
            Self::ProxyMode(value) => Some(VisibleValue::ProxyMode(*value)),
            Self::Secret(_) => None,
        }
    }
}

/// Validates syntax without echoing raw input in an error.
pub(crate) fn parse_value(key: &ConfigKey, raw: &str) -> Result<ConfigValue, ConfigError> {
    if raw.len() > 32 * 1024 || raw.contains('\0') {
        return Err(ConfigError::InvalidValue("NUL or value over 32 KiB"));
    }
    match key.spec().kind {
        ValueKind::Boolean => match raw {
            "true" => Ok(ConfigValue::Boolean(true)),
            "false" => Ok(ConfigValue::Boolean(false)),
            _ => Err(ConfigError::InvalidValue("expected true or false")),
        },
        ValueKind::GraceSeconds => raw
            .parse::<u16>()
            .ok()
            .filter(|value| *value <= 3600 && raw.bytes().all(|b| b.is_ascii_digit()))
            .map(ConfigValue::Integer)
            .ok_or(ConfigError::InvalidValue("expected integer from 0 to 3600")),
        ValueKind::RetryAttempts => raw
            .parse::<u16>()
            .ok()
            .filter(|value| *value > 0 && raw.bytes().all(|b| b.is_ascii_digit()))
            .map(ConfigValue::Integer)
            .ok_or(ConfigError::InvalidValue(
                "expected integer from 1 to 65535",
            )),
        ValueKind::ExecutionMode => match raw {
            "sandboxed" | "unrestricted" => Ok(ConfigValue::Text(raw.into())),
            _ => Err(ConfigError::InvalidValue(
                "expected sandboxed or unrestricted",
            )),
        },
        ValueKind::Environment => {
            if let ConfigKey::Environment(name) = key {
                if name.is_proxy_address() {
                    validate_proxy(raw)?;
                }
                if name.is_proxy() && raw.chars().any(char::is_control) {
                    return Err(ConfigError::InvalidValue(
                        "control character in proxy setting",
                    ));
                }
            }
            Ok(ConfigValue::Text(raw.to_owned()))
        }
        ValueKind::ProxyMode => match raw {
            "inherit" => Ok(ConfigValue::ProxyMode(ProxyMode::Inherit)),
            "custom" => Ok(ConfigValue::ProxyMode(ProxyMode::Custom)),
            "direct" => Ok(ConfigValue::ProxyMode(ProxyMode::Direct)),
            _ => Err(ConfigError::InvalidValue(
                "expected inherit, custom or direct",
            )),
        },
        ValueKind::Identity => Err(ConfigError::ReadOnlySetting),
    }
}

pub(crate) fn requires_secret(key: &ConfigKey, raw: &str) -> bool {
    match key {
        ConfigKey::Environment(name) => {
            name.is_sensitive()
                || (name.is_proxy_address()
                    && raw
                        .split("://")
                        .nth(1)
                        .and_then(|rest| rest.split('/').next())
                        .is_some_and(|authority| authority.contains('@')))
        }
        _ => false,
    }
}

pub(crate) fn validate_proxy(raw: &str) -> Result<(), ConfigError> {
    let invalid = || {
        ConfigError::InvalidValue(
            "expected an HTTP(S) or SOCKS5 proxy URL without path, query or fragment",
        )
    };
    if raw
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
        || !raw.contains("://")
    {
        return Err(invalid());
    }
    let url = Url::parse(raw).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h")
        || url.host_str().is_none()
        || url.port() == Some(0)
        || !matches!(url.path(), "" | "/")
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(())
}
