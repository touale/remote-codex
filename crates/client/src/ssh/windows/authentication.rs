use super::{Handle, config::Config, failure};
use crate::{
    Result,
    connection::SshEndpoint,
    ssh::{
        ConnectOptions,
        interaction::{AuthenticationHandler, AuthenticationKind, AuthenticationPrompt},
    },
};
use russh::keys::{PrivateKeyWithHashAlg, load_secret_key};
use std::sync::Arc;
use zeroize::Zeroizing;

pub(super) async fn prompt(
    handler: Option<&AuthenticationHandler>,
    target: &str,
    kind: AuthenticationKind,
    message: String,
) -> Result<Zeroizing<String>> {
    if let Some(handler) = handler
        && let Some(value) = handler(AuthenticationPrompt {
            target: target.into(),
            kind,
            message,
        })
        .await?
    {
        return Ok(value);
    }
    Err(remote_codex_protocol::Fault::new(
        "SSH_AUTH_REQUIRED",
        "SSH authentication is required. Connect interactively to continue.",
    )
    .into())
}

pub(super) async fn authenticate(
    handle: &mut Handle,
    config: &Config,
    options: &ConnectOptions,
    endpoint: &SshEndpoint,
) -> Result<()> {
    let target = &config.target;
    let saved = match &options.credentials {
        Some(credentials) => credentials
            .resolve(options, endpoint)
            .await?
            .map(|(_, value)| value),
        None => None,
    };
    if let Some(password) = &saved
        && handle
            .authenticate_password(&target.user, password.as_str())
            .await
            .map_err(failure)?
            .success()
    {
        return Ok(());
    }
    for path in &config.identities {
        if !path.is_file() {
            continue;
        }
        remote_codex_platform::fs::check_private(path)?;
        let key = match load_secret_key(path, None) {
            Ok(key) => key,
            Err(russh::keys::Error::KeyIsEncrypted)
            | Err(russh::keys::Error::SshKey(russh::keys::ssh_key::Error::Encrypted)) => {
                let passphrase = prompt(
                    options.interaction.as_ref(),
                    &target.hostname,
                    AuthenticationKind::Passphrase,
                    format!("Passphrase for {}", path.display()),
                )
                .await?;
                load_secret_key(path, Some(&passphrase)).map_err(invalid_key)?
            }
            Err(error) => return Err(invalid_key(error)),
        };
        if !matches!(
            key.algorithm(),
            russh::keys::ssh_key::Algorithm::Ed25519 | russh::keys::ssh_key::Algorithm::Rsa { .. }
        ) {
            return Err(crate::ClientError::Unsupported(
                "Windows SSH supports RSA and Ed25519 private keys",
            ));
        }
        let hash = handle
            .best_supported_rsa_hash()
            .await
            .map_err(failure)?
            .flatten();
        if handle
            .authenticate_publickey(
                &target.user,
                PrivateKeyWithHashAlg::new(Arc::new(key), hash),
            )
            .await
            .map_err(failure)?
            .success()
        {
            return Ok(());
        }
    }
    for _ in 0..3 {
        let password = prompt(
            options.interaction.as_ref(),
            &target.hostname,
            AuthenticationKind::Password,
            target.password_prompt(),
        )
        .await?;
        if handle
            .authenticate_password(&target.user, password.as_str())
            .await
            .map_err(failure)?
            .success()
        {
            return Ok(());
        }
    }
    Err(remote_codex_protocol::Fault::new(
        "SSH_AUTH_REQUIRED",
        "SSH authentication failed. Check the saved password or identity.",
    )
    .into())
}

fn invalid_key(error: russh::keys::Error) -> crate::ClientError {
    remote_codex_protocol::Fault::new(
        "SSH_IDENTITY_REQUIRED",
        &format!("Could not read or decrypt the SSH private key: {error}"),
    )
    .into()
}
