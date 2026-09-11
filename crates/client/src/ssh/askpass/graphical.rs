use super::*;
use crate::ssh::interaction::{AuthenticationHandler, AuthenticationKind, AuthenticationPrompt};
use base64::Engine;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};

pub(super) async fn prepare(
    options: &ConnectOptions,
    endpoint: &SshEndpoint,
    command: &mut Command,
    handler: AuthenticationHandler,
) -> Result<Askpass> {
    let target = crate::ssh::target::Target::resolve(options, endpoint).await?;
    let expected = target.password_prompt();
    let saved = match &options.credentials {
        Some(c) => c.resolve(options, endpoint).await?,
        None => None,
    };
    let password_only = saved.is_some();
    let directory = tempfile::Builder::new()
        .prefix("rc-gui-auth-")
        .tempdir_in("/tmp")?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
    let token = uuid::Uuid::new_v4().to_string();
    let listener = UnixListener::bind(directory.path().join("prompt"))?;
    let broker_token = token.clone();
    let target_label = target.binding()?;
    let broker = broker::Broker::task(tokio::spawn(async move {
        let mut saved = saved.map(|(_, password)| password);
        for _ in 0..8 {
            let attempt = async {
                let (mut stream, _) = listener.accept().await?;
                if stream.peer_cred()?.uid() != nix::unistd::geteuid().as_raw() {
                    return std::io::Result::Ok(());
                }
                let mut request = String::new();
                let mut reader = BufReader::new((&mut stream).take(16384));
                reader.read_line(&mut request).await?;
                if request.trim_end() != broker_token {
                    return Ok(());
                }
                request.clear();
                reader.read_line(&mut request).await?;
                drop(reader);
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(request.trim())
                    .map_err(std::io::Error::other)?;
                let message = String::from_utf8(bytes).map_err(std::io::Error::other)?;
                let answer = if message == expected && saved.is_some() {
                    saved.take()
                } else {
                    let kind = if message == expected {
                        AuthenticationKind::Password
                    } else if message.ends_with(
                        "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
                    ) {
                        AuthenticationKind::HostKey
                    } else if message.starts_with("Enter passphrase for key '") {
                        AuthenticationKind::Passphrase
                    } else {
                        return Ok(());
                    };
                    let host = matches!(kind, AuthenticationKind::HostKey);
                    let answer = handler(AuthenticationPrompt {
                        target: target_label.clone(),
                        kind,
                        message,
                    })
                    .await
                    .map_err(std::io::Error::other)?;
                    if host {
                        answer.filter(|a| a.as_str() == "yes")
                    } else {
                        answer
                    }
                };
                if let Some(answer) = answer {
                    if answer.contains(['\n', '\r', '\0']) || answer.len() > 4096 {
                        return Ok(());
                    }
                    stream.write_all(answer.as_bytes()).await?;
                    stream.write_all(b"\n").await?;
                }
                stream.shutdown().await
            };
            if tokio::time::timeout(std::time::Duration::from_secs(180), attempt)
                .await
                .is_err()
            {
                break;
            }
        }
    }));
    let script = format!(
        "#!/bin/sh\nset -eu\nRC_ROOT={}\nRC_TOKEN={}\n{}",
        quote(directory.path().to_str().ok_or(ClientError::PrivatePath)?)?,
        quote(&token)?,
        include_str!("graphical.sh")
    );
    let path = directory.path().join("askpass");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&path)?
        .write_all(script.as_bytes())?;
    target.pin(command);
    command
        .env("SSH_ASKPASS", path)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env_remove("SSH_ASKPASS_PROMPT")
        .args(["-o", "BatchMode=no", "-o", "NumberOfPasswordPrompts=1"]);
    if password_only {
        command.args([
            "-o",
            "PreferredAuthentications=password",
            "-o",
            "PasswordAuthentication=yes",
        ]);
    }
    Ok(Askpass {
        _broker: broker,
        directory,
    })
}
