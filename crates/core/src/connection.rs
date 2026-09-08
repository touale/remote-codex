use std::net::IpAddr;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EndpointError {
    #[error("invalid SSH target; expected host, user@host or user@[IPv6]")]
    InvalidTarget,
    #[error("SSH port must be between 1 and 65535")]
    InvalidPort,
    #[error("connection name must contain 1–128 visible characters and cannot start with '-' ")]
    InvalidName,
}

/// Unspecified user and port stay unspecified so OpenSSH Host rules still apply.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SshEndpoint {
    host: String,
    user: Option<String>,
    port: Option<u16>,
}

impl SshEndpoint {
    pub fn parse(target: &str, port: Option<u16>) -> Result<Self, EndpointError> {
        if port == Some(0) {
            return Err(EndpointError::InvalidPort);
        }
        let (user, host) = match target.split_once('@') {
            Some((user, host)) if valid_word(user) => (Some(user.to_owned()), host),
            Some(_) => return Err(EndpointError::InvalidTarget),
            None => (None, target),
        };
        let host = if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            inner
                .parse::<std::net::Ipv6Addr>()
                .map_err(|_| EndpointError::InvalidTarget)?
                .to_string()
        } else if let Ok(address) = host.parse::<IpAddr>() {
            address.to_string()
        } else if valid_word(host) {
            host.to_owned()
        } else {
            return Err(EndpointError::InvalidTarget);
        };
        Ok(Self { host, user, port })
    }

    pub fn validate(&self) -> Result<(), EndpointError> {
        let parsed = Self::parse(&self.destination(), self.port)?;
        if parsed == *self {
            Ok(())
        } else {
            Err(EndpointError::InvalidTarget)
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }
    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn destination(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        match &self.user {
            Some(user) => format!("{user}@{host}"),
            None => host,
        }
    }

    pub fn default_name(&self) -> String {
        match self.port {
            Some(port) => format!("{}:{port}", self.destination()),
            None => self.destination(),
        }
    }
}

pub fn validate_name(name: &str) -> Result<(), EndpointError> {
    if name.is_empty()
        || name.len() > 128
        || name.starts_with('-')
        || name.trim() != name
        || name.chars().any(char::is_control)
    {
        return Err(EndpointError::InvalidName);
    }
    Ok(())
}

fn valid_word(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= 253
        && !word.starts_with('-')
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
