use super::ConfigError;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnvironmentName(String);

impl EnvironmentName {
    pub fn parse(name: &str) -> Result<Self, ConfigError> {
        let mut bytes = name.bytes();
        if !matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
            || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || name.len() > 128
        {
            return Err(ConfigError::InvalidEnvironmentName);
        }
        let upper = name.to_ascii_uppercase();
        if upper.starts_with("REMOTE_CODEX_")
            || matches!(
                upper.as_str(),
                "CODEX_HOME"
                    | "OPENAI_API_KEY"
                    | "CODEX_API_KEY"
                    | "CODEX_ACCESS_TOKEN"
                    | "CODEX_AUTH_JSON"
                    | "HOME"
                    | "USER"
                    | "LOGNAME"
                    | "XDG_CONFIG_HOME"
                    | "XDG_DATA_HOME"
                    | "XDG_RUNTIME_DIR"
            )
        {
            return Err(ConfigError::ReservedEnvironment);
        }
        let name = if PROXY_NAMES.contains(&name.to_ascii_lowercase().as_str()) {
            name.to_ascii_lowercase()
        } else {
            name.to_owned()
        };
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_proxy(&self) -> bool {
        PROXY_NAMES.contains(&self.as_str())
    }

    pub fn is_proxy_address(&self) -> bool {
        self.is_proxy() && self.as_str() != "no_proxy"
    }

    pub fn is_sensitive(&self) -> bool {
        let name = self.0.to_ascii_uppercase();
        name.split('_').any(|part| {
            matches!(
                part,
                "TOKEN" | "SECRET" | "PASSWORD" | "PASSWD" | "CREDENTIAL"
            )
        }) || name.ends_with("_KEY")
            || name == "KEY"
    }
}

pub(crate) const PROXY_NAMES: [&str; 4] = ["http_proxy", "https_proxy", "all_proxy", "no_proxy"];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigKey {
    Background,
    DisconnectGraceSeconds,
    ReconnectMaxAttempts,
    Environment(EnvironmentName),
    ProxyMode,
    ExecutionMode,
    SshHost,
    SshUser,
    SshPort,
}

impl ConfigKey {
    pub fn parse(key: &str) -> Result<Self, ConfigError> {
        match key {
            "background" => Ok(Self::Background),
            "disconnect_grace_seconds" => Ok(Self::DisconnectGraceSeconds),
            "reconnect.max_attempts" => Ok(Self::ReconnectMaxAttempts),
            "proxy.mode" => Ok(Self::ProxyMode),
            "execution.mode" => Ok(Self::ExecutionMode),
            "ssh.host" => Ok(Self::SshHost),
            "ssh.user" => Ok(Self::SshUser),
            "ssh.port" => Ok(Self::SshPort),
            _ => key
                .strip_prefix("env.")
                .ok_or(ConfigError::UnknownKey)
                .and_then(EnvironmentName::parse)
                .map(Self::Environment),
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::Background => "background",
            Self::DisconnectGraceSeconds => "disconnect_grace_seconds",
            Self::ReconnectMaxAttempts => "reconnect.max_attempts",
            Self::Environment(name) => return format!("env.{}", name.as_str()),
            Self::ProxyMode => "proxy.mode",
            Self::ExecutionMode => "execution.mode",
            Self::SshHost => "ssh.host",
            Self::SshUser => "ssh.user",
            Self::SshPort => "ssh.port",
        }
        .to_owned()
    }
}
