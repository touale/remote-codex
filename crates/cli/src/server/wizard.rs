use crate::{
    args::{AddArgs, Cli},
    ui,
};
use remote_codex_client::{
    ClientError, Result,
    config::{ConfigInput, ConfigKey, ConfigLayer, ConfigValue, resolve},
    store::LocalStore,
};

pub(crate) struct Settings {
    pub(crate) name: String,
    pub(crate) address: String,
    pub(crate) changes: Vec<(String, String)>,
    pub(crate) install_key: bool,
    pub(crate) interactive: bool,
    pub(crate) port: Option<u16>,
}

pub(crate) async fn collect(cli: &Cli, store: &LocalStore, args: &AddArgs) -> Result<Settings> {
    let interactive = ui::interactive() && !cli.json && !args.non_interactive;
    let name = match &cli.connection {
        Some(name) => name.clone(),
        None if interactive => ui::input("Server name")?,
        None => return Err(ClientError::Argument("server add requires -n NAME")),
    };
    let existing = store.find_connection(&name).await.ok();
    let address = match &args.addr {
        Some(addr) => addr.clone(),
        None => match &existing {
            Some(record) => {
                let host = if record.endpoint.host().contains(':') {
                    format!("[{}]", record.endpoint.host())
                } else {
                    record.endpoint.host().to_owned()
                };
                record
                    .endpoint
                    .user()
                    .map(|u| format!("{u}@{host}"))
                    .unwrap_or(host)
            }
            None if interactive => ui::input("SSH address (user@host)")?,
            None => {
                return Err(ClientError::Argument(
                    "server add requires --addr USER@HOST",
                ));
            }
        },
    };
    let port = args.port.or_else(|| {
        if args.addr.is_none() {
            existing.as_ref().and_then(|r| r.endpoint.port())
        } else {
            None
        }
    });
    let effective = match &existing {
        Some(record) => store.config_snapshot(&record.id).await?.effective,
        None => resolve(&ConfigLayer::new())?,
    };
    let mut changes = Vec::new();
    let mut mode = args.proxy_mode.clone();
    let mut proxy = args.proxy.clone();
    if interactive && mode.is_none() && proxy.is_none() && existing.is_none() {
        let choice = ui::choose(
            &format!("Proxy settings for {}", ui::text(&name)),
            &["Connect directly".into(), "Use a proxy".into()],
        )?;
        if choice == 0 {
            mode = Some("direct".into());
        } else {
            mode = Some("custom".into());
            proxy = Some(proxy_input()?);
        }
    }
    if let Some(proxy) = proxy {
        mode.get_or_insert("custom".into());
        if proxy.starts_with("socks") {
            changes.push(("env.all_proxy".into(), proxy));
        } else {
            changes.push(("env.http_proxy".into(), proxy.clone()));
            changes.push(("env.https_proxy".into(), proxy));
        }
    }
    if let Some(mode) = mode {
        changes.push(("proxy.mode".into(), mode));
    }
    if args.background {
        changes.push(("background".into(), "true".into()));
    } else if args.no_background {
        changes.push(("background".into(), "false".into()));
    } else if interactive && existing.is_none() {
        let default = matches!(
            effective.get(&ConfigKey::Background),
            Some(ConfigValue::Boolean(true)),
        );
        let background = ui::confirm(
            "Keep submitted commands running after you disconnect?",
            default,
        )?;
        changes.push(("background".into(), background.to_string()));
    }
    if let Some(mode) = &args.execution_mode {
        changes.push(("execution.mode".into(), mode.clone()));
    }
    for setting in &args.environment {
        let (key, value) = setting
            .split_once('=')
            .ok_or(ClientError::Argument("--env expects NAME=VALUE"))?;
        changes.push((format!("env.{key}"), value.into()));
    }
    let install_key = args.install_key
        || (interactive
            && existing.is_none()
            && args.identity.is_none()
            && ui::confirm(
                "Install a dedicated SSH key? Existing SSH keys/agent can also be used.",
                false,
            )?);
    Ok(Settings {
        name,
        address,
        changes,
        install_key,
        interactive,
        port,
    })
}

fn proxy_input() -> Result<String> {
    ui::validated_input("Proxy address (e.g. http://proxy.example:7890)", |value| {
        let key = if value.starts_with("socks") {
            "env.all_proxy"
        } else {
            "env.https_proxy"
        };
        let mut layer = ConfigLayer::new();
        layer
            .set(key, ConfigInput::Plain(value))
            .map_err(|error| error.to_string())
    })
}
