use remote_codex_client::{
    ClientError,
    application::{AuthenticationHandler, AuthenticationKind},
};
use std::sync::Arc;
use zeroize::Zeroizing;
pub(crate) fn handler() -> AuthenticationHandler {
    Arc::new(|prompt| {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let value = match prompt.kind {
                    AuthenticationKind::HostKey => dialoguer::Input::<String>::new()
                        .with_prompt(prompt.message)
                        .interact_text(),
                    AuthenticationKind::Password | AuthenticationKind::Passphrase => {
                        dialoguer::Password::new()
                            .with_prompt(prompt.message)
                            .interact()
                    }
                }
                .map_err(|_| ClientError::Argument("SSH authentication cancelled"))?;
                Ok(Some(Zeroizing::new(value)))
            })
            .await
            .map_err(|_| ClientError::Argument("SSH authentication cancelled"))?
        })
    })
}
