use crate::{
    args::{AddArgs, Cli},
    ui,
};
use remote_codex_client::{
    ClientError, Result,
    application::{AddServer, Client},
    config::{ConfigInput, ConfigLayer},
};

pub(crate) async fn collect(cli: &Cli, client: &Client, args: &AddArgs) -> Result<AddServer> {
    let interactive = ui::interactive() && !cli.json && !args.non_interactive;
    let name = match &cli.connection {
        Some(name) => name.clone(),
        None if interactive => ui::input("Server name")?,
        None => return Err(ClientError::Argument("server add requires -n NAME")),
    };
    let existing = client.servers().find(&name).await.ok();
    let address = match &args.addr {
        Some(addr) => addr.clone(),
        None => match &existing {
            Some(record) => record.endpoint.destination(),
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
    if let Some(mode) = &args.execution_mode {
        changes.push(("execution.mode".into(), mode.clone()));
    }
    for setting in &args.environment {
        let (key, value) = setting
            .split_once('=')
            .ok_or(ClientError::Argument("--env expects NAME=VALUE"))?;
        changes.push((format!("env.{key}"), value.into()));
    }
    let mut save_password = args.save_password || args.password_stdin;
    if interactive
        && existing.is_none()
        && args.identity.is_none()
        && !args.install_key
        && !save_password
    {
        save_password = ui::choose(
            "SSH authentication",
            &[
                "Save password in macOS Keychain".into(),
                "Use SSH keys or enter password when prompted".into(),
            ],
        )? == 0;
    }
    if save_password && !args.password_stdin && !interactive {
        return Err(ClientError::Argument(
            "saving an SSH password requires an interactive terminal or --password-stdin",
        ));
    }
    let password = save_password
        .then(|| super::authentication::password(args.password_stdin))
        .transpose()?;
    Ok(AddServer {
        name,
        address,
        settings: changes,
        identity: args.identity.clone(),
        install_key: args.install_key,
        port,
        password,
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
