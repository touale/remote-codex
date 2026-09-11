use super::{ConnectOptions, quote};
use crate::{ClientError, Result, connection::SshEndpoint};
use std::{
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
};
use tokio::process::Command;
mod broker;
mod graphical;

pub(super) struct Askpass {
    _broker: broker::Broker,
    directory: tempfile::TempDir,
}

impl Askpass {
    pub(super) async fn prepare(
        options: &ConnectOptions,
        endpoint: &SshEndpoint,
        interactive: bool,
        command: &mut Command,
    ) -> Result<Option<Self>> {
        if let Some(handler) = &options.interaction {
            return graphical::prepare(options, endpoint, command, handler.clone())
                .await
                .map(Some);
        }
        let Some(credentials) = &options.credentials else {
            return Ok(None);
        };
        let Some((target, password)) = credentials.resolve(options, endpoint).await? else {
            return Ok(None);
        };
        let prompt = target.password_prompt();
        let directory = tempfile::Builder::new()
            .prefix("rc-auth-")
            .tempdir_in("/tmp")?;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
        crate::store::private_directory(directory.path())?;
        let token = uuid::Uuid::new_v4().to_string();
        let broker =
            broker::Broker::start(&directory.path().join("password"), token.clone(), password)?;
        let script = format!(
            "#!/bin/sh\nRC_ASKPASS_ROOT={}\nRC_ASKPASS_TOKEN={}\nRC_ASKPASS_PASSWORD_PROMPT={}\nRC_ASKPASS_INTERACTIVE={}\n{}",
            quote(directory.path().to_str().ok_or(ClientError::PrivatePath)?)?,
            quote(&token)?,
            quote(&prompt)?,
            u8::from(interactive),
            include_str!("askpass.sh"),
        );
        let path = directory.path().join("askpass");
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o700)
            .open(&path)?;
        file.write_all(script.as_bytes())?;
        target.pin(command);
        command
            .env("SSH_ASKPASS", &path)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env_remove("SSH_ASKPASS_PROMPT")
            .args([
                "-o",
                "BatchMode=no",
                "-o",
                "NumberOfPasswordPrompts=1",
                "-o",
                "PreferredAuthentications=password",
                "-o",
                "PasswordAuthentication=yes",
            ]);
        Ok(Some(Self {
            _broker: broker,
            directory,
        }))
    }

    pub(super) fn bind(&self, pid: u32) -> Result<()> {
        std::fs::write(self.directory.path().join("owner"), pid.to_string())?;
        Ok(())
    }

    pub(super) fn failed(&self) -> bool {
        self.directory.path().join("vault-error").exists()
    }
}

#[cfg(test)]
mod tests;
