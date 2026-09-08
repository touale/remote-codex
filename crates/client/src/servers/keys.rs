use crate::{
    ClientError, Result,
    connection::SshEndpoint,
    ssh::{SshTransport, quote},
    store::{ServerAccess, private_directory},
};
use std::{path::Path, process::Stdio};
use tokio::process::Command;

pub(crate) async fn install(
    ssh: &SshTransport,
    endpoint: &SshEndpoint,
    directory: &Path,
    id: &str,
    access: &mut ServerAccess,
) -> Result<()> {
    let root = directory.join("keys");
    private_directory(&root)?;
    let key = root.join(id);
    if !key.exists() {
        let status = Command::new("ssh-keygen")
            .args(["-t", "ed25519", "-C"])
            .arg(format!("remote-codex:{id}"))
            .arg("-f")
            .arg(&key)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await?;
        if !status.success() {
            return Err(ClientError::Argument("SSH key generation did not complete"));
        }
    }
    let public = tokio::fs::read_to_string(key.with_extension("pub(crate)"))
        .await?
        .trim()
        .to_owned();
    if !public.starts_with("ssh-ed25519 ")
        || public.contains(['\r', '\n'])
        || !public.ends_with(&format!("remote-codex:{id}"))
    {
        return Err(ClientError::Argument(
            "managed SSH public key has an unexpected identity",
        ));
    }
    let script = format!(
        "RC_PUBLIC={}\nRC_REVOKE=0\n{}",
        quote(&public)?,
        include_str!("key.sh")
    );
    ssh.script(endpoint, &script).await?;
    access.identity_file = Some(key.clone());
    access.managed_public_key = Some(public);
    if std::env::var_os("SSH_AUTH_SOCK").is_some() {
        let mut add = Command::new("ssh-add");
        #[cfg(target_os = "macos")]
        add.arg("--apple-use-keychain");
        let status = add
            .arg(&key)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .await?;
        if !status.success() {
            return Err(ClientError::Argument(
                "key installed; unlock it with ssh-add before automatic connections",
            ));
        }
    }
    Ok(())
}

pub(crate) async fn revoke(
    ssh: &SshTransport,
    endpoint: &SshEndpoint,
    access: &ServerAccess,
) -> Result<()> {
    let public = access
        .managed_public_key
        .as_deref()
        .ok_or(ClientError::Argument(
            "this server has no product-managed key to revoke",
        ))?;
    ssh.script(
        endpoint,
        &format!(
            "RC_PUBLIC={}\nRC_REVOKE=1\n{}",
            quote(public)?,
            include_str!("key.sh")
        ),
    )
    .await?;
    Ok(())
}
