use crate::{
    ClientError, Result,
    connection::SshEndpoint,
    ssh::{SshTransport, quote},
    store::{LocalStore, ServerAccess, private_directory},
};
use std::path::Path;
#[cfg(unix)]
use std::process::Stdio;
#[cfg(unix)]
use tokio::process::Command;

pub(crate) async fn install(
    ssh: &SshTransport,
    endpoint: &SshEndpoint,
    directory: &Path,
    id: &str,
    store: &LocalStore,
) -> Result<()> {
    let root = directory.join("keys");
    private_directory(&root)?;
    let key = root.join(id);
    #[cfg(unix)]
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
    #[cfg(windows)]
    if !key.exists() {
        use russh::keys::ssh_key::{LineEnding, PrivateKey, private::Ed25519Keypair};
        use std::io::Write;
        let mut seed = zeroize::Zeroizing::new([0u8; 32]);
        getrandom::fill(seed.as_mut()).map_err(|_| ClientError::Credentials)?;
        let mut secret = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
        secret.set_comment(format!("remote-codex:{id}"));
        let mut file = remote_codex_platform::fs::open_private(
            &key,
            remote_codex_platform::fs::Creation::New,
        )?;
        file.write_all(
            secret
                .to_openssh(LineEnding::LF)
                .map_err(|_| ClientError::Credentials)?
                .as_bytes(),
        )?;
        file.sync_all()?;
        tokio::fs::write(
            key.with_extension("pub"),
            secret
                .public_key()
                .to_openssh()
                .map_err(|_| ClientError::Credentials)?,
        )
        .await?;
    }
    let public = tokio::fs::read_to_string(key.with_extension("pub"))
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
    store.set_managed_key(id, &key, &public).await?;
    #[cfg(unix)]
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
