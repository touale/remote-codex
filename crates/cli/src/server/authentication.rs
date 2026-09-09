use crate::{args::Cli, context::Context, output, ui};
use remote_codex_client::{ClientError, Result};
use std::io::{IsTerminal, Read};
use zeroize::Zeroizing;

pub(super) fn password(stdin: bool) -> Result<Zeroizing<String>> {
    let value = if stdin {
        if std::io::stdin().is_terminal() {
            return Err(ClientError::Argument(
                "--password-stdin requires piped input; omit it for a hidden password prompt",
            ));
        }
        let mut value = Zeroizing::new(String::new());
        std::io::stdin().take(1026).read_to_string(&mut value)?;
        if value.ends_with('\n') {
            value.pop();
            if value.ends_with('\r') {
                value.pop();
            }
        }
        value
    } else {
        if !ui::interactive() {
            return Err(ClientError::Argument(
                "saving an SSH password requires a terminal or --password-stdin",
            ));
        }
        Zeroizing::new(
            dialoguer::Password::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt("SSH password (saved in macOS Keychain)")
                .report(false)
                .interact()
                .map_err(|_| ClientError::Argument("password input cancelled"))?,
        )
    };
    remote_codex_client::application::validate_ssh_password(&value)?;
    Ok(value)
}

pub(super) async fn run(cli: &Cli, context: &Context, forget: bool, stdin: bool) -> Result<()> {
    if cli.json && !forget && !stdin {
        return Err(ClientError::Argument(
            "server auth --json requires --password-stdin or --forget",
        ));
    }
    let name = cli
        .connection
        .as_deref()
        .ok_or(ClientError::Argument("server auth requires -n NAME"))?;
    context.client.servers().find(name).await?;
    if forget {
        context.client.servers().forget_password(name).await?;
    } else {
        let password = password(stdin)?;
        context
            .prepare(context.client.servers().save_password(name, password))
            .await?;
    }
    if cli.json {
        output::json(&serde_json::json!({"server": name, "password_saved": !forget}))
    } else {
        println!(
            "SSH password {} for {}.",
            if forget {
                "removed"
            } else {
                "saved in macOS Keychain"
            },
            ui::text(name)
        );
        Ok(())
    }
}
