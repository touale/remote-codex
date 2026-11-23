use super::super::{ConnectOptions, target::Target};
use crate::{ClientError, Result, connection::SshEndpoint};
use std::{io::Cursor, path::PathBuf};

pub(in crate::ssh) struct Config {
    pub target: Target,
    pub identities: Vec<PathBuf>,
    pub known_hosts: PathBuf,
}

impl Config {
    pub(in crate::ssh) fn load(options: &ConnectOptions, endpoint: &SshEndpoint) -> Result<Self> {
        let home = remote_codex_platform::fs::home_dir()?;
        let path = options
            .config
            .clone()
            .unwrap_or_else(|| home.join(".ssh/config"));
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && options.config.is_none() => {
                String::new()
            }
            Err(e) => return Err(e.into()),
        };
        // Filter unsupported options only in unrelated Host blocks. Match/Include
        // change evaluation itself and cannot be safely treated as inert settings.
        let mut active = true;
        let mut filtered = String::new();
        for line in source.lines() {
            let (name, value) = line
                .trim()
                .split_once(|c: char| c.is_whitespace() || c == '=')
                .unwrap_or((line.trim(), ""));
            let name = name.to_ascii_lowercase();
            if name == "host" {
                let patterns = shlex::split(value.trim_start_matches('=').trim())
                    .ok_or(ClientError::Argument("Invalid SSH Host pattern"))?;
                let clauses = patterns
                    .into_iter()
                    .map(|pattern| {
                        ssh2_config::HostClause::new(
                            pattern.trim_start_matches('!').into(),
                            pattern.starts_with('!'),
                        )
                    })
                    .collect();
                active = ssh2_config::Host::new(
                    clauses,
                    ssh2_config::HostParams::new(&Default::default()),
                )
                .intersects(endpoint.host());
            }
            if !name.is_empty()
                && !name.starts_with('#')
                && !matches!(
                    name.as_str(),
                    "host" | "hostname" | "user" | "port" | "identityfile"
                )
            {
                if active || matches!(name.as_str(), "match" | "include") {
                    return Err(remote_codex_protocol::Fault::new("SSH_CONFIG_UNSUPPORTED", &format!("Windows SSH does not support the '{name}' directive. Use a basic Host configuration or a separate --ssh-config file.")).into());
                }
                continue;
            }
            filtered.push_str(line);
            filtered.push('\n');
        }
        let config = ssh2_config::SshConfig::default()
            .home_dir(&home)
            .parse(&mut Cursor::new(filtered), ssh2_config::ParseRule::STRICT)
            .map_err(|_| ClientError::Argument("Invalid SSH configuration"))?;
        let found = config.query(endpoint.host());
        let target = Target {
            hostname: found.host_name.unwrap_or_else(|| endpoint.host().into()),
            user: endpoint
                .user()
                .map(str::to_owned)
                .or(found.user)
                .or_else(|| std::env::var("USERNAME").ok())
                .ok_or(ClientError::Argument("SSH user is required"))?,
            port: endpoint.port().or(found.port).unwrap_or(22),
        };
        let identities = options
            .identity_file
            .clone()
            .map(|p| vec![p])
            .or(found.identity_file)
            .unwrap_or_else(|| vec![home.join(".ssh/id_ed25519"), home.join(".ssh/id_rsa")]);
        Ok(Self {
            target,
            identities,
            known_hosts: home.join(".ssh/known_hosts"),
        })
    }
}
