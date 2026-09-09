use crate::Result;
use std::process::Command;

#[test]
fn password_is_only_released_for_owned_master_and_exact_target_prompt() -> Result<()> {
    let root = tempfile::tempdir()?;
    let owner = root.path().join("owner");
    std::fs::write(&owner, std::process::id().to_string())?;
    let script = include_str!("../askpass.sh").replace(
        "printf '%s\\n' \"$RC_ASKPASS_TOKEN\" | /usr/bin/nc -w 5 -U \"$RC_ASKPASS_ROOT/password\"",
        "printf '%s' 'synthetic-test-password'",
    );
    let prompt = "user@host's password: ";
    let invoke = |message: &str| -> Result<std::process::Output> {
        Ok(Command::new("/bin/sh")
            .args(["-c", &script, "askpass", message])
            .env("RC_ASKPASS_ROOT", root.path())
            .env("RC_ASKPASS_PASSWORD_PROMPT", prompt)
            .env("RC_ASKPASS_INTERACTIVE", "0")
            .output()?)
    };
    let accepted = invoke(prompt)?;
    assert!(accepted.status.success());
    assert_eq!(accepted.stdout, b"synthetic-test-password");
    for denied in [
        "user@other-host's password: ",
        "another@host's password: ",
        "Enter passphrase for key '/keys/key': ",
        "Verification code: ",
        "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
        "user@host's password: $(touch /tmp/invalid-askpass-injection)",
    ] {
        let result = invoke(denied)?;
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
    }
    std::fs::write(owner, "0")?;
    assert!(
        invoke(prompt)?.stdout.is_empty(),
        "ProxyJump children must not receive the target password"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires access to the native macOS Keychain"]
async fn native_keychain_password_reaches_askpass_without_a_plaintext_file() -> Result<()> {
    use crate::{
        config::SecretRef,
        connection::SshEndpoint,
        credentials::{CredentialVault, NativeVault},
        ssh::{ConnectOptions, credentials::Credentials},
        store::LocalStore,
    };
    let root = tempfile::tempdir()?;
    let store = LocalStore::open(&root.path().join("state")).await?;
    let endpoint = SshEndpoint::parse("fixture@example.invalid", None)?;
    let record = store.save_connection(&endpoint, Some("fixture")).await?;
    let reference = SecretRef::new(&uuid::Uuid::new_v4().to_string())?;
    let vault = NativeVault::new(&store.installation_id().await?);
    let password = zeroize::Zeroizing::new(format!("test-{}", uuid::Uuid::new_v4()));
    vault
        .put(
            &reference,
            &password,
            crate::credentials::locking::write(&store.directory).await?,
        )
        .await?;
    let result = async {
        let options = ConnectOptions {
            credentials: Some(Credentials::pending(&store, &record.id, reference.clone())),
            ..Default::default()
        };
        let target = crate::ssh::target::Target::resolve(&options, &endpoint)
            .await?
            .binding()?;
        store
            .reserve_ssh_credential(&record.id, &reference, &target)
            .await?;
        let mut command = tokio::process::Command::new("ssh");
        let askpass = super::Askpass::prepare(&options, &endpoint, false, &mut command)
            .await?
            .ok_or(crate::ClientError::Credentials)?;
        askpass.bind(std::process::id())?;
        let path = askpass.directory.path().join("askpass");
        assert!(!std::fs::read_to_string(&path)?.contains(password.as_str()));
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            tokio::process::Command::new(&path)
                .arg("fixture@example.invalid's password: ")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| crate::ClientError::Timeout)??;
        if !output.status.success()
            || output.stdout.strip_suffix(b"\n").unwrap_or(&output.stdout) != password.as_bytes()
        {
            return Err(crate::ClientError::Credentials);
        }
        Ok(())
    }
    .await;
    let cleanup = vault.delete(&reference).await;
    store.close().await;
    cleanup?;
    result
}
