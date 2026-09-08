use remote_codex_core::config::{
    ConfigError, ConfigInput, ConfigLayer, EnvironmentName, SecretRef, resolve,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn secrets_and_proxy_credentials_require_credential_storage() {
    let mut local = ConfigLayer::new();
    for (key, value) in [
        ("env.SERVICE_API_KEY", "credential"),
        ("env.GITHUB_TOKEN", "credential"),
        ("env.PASSWORD", "credential"),
        (
            "env.https_proxy",
            "http://alice:credential@proxy.example:7890",
        ),
        ("env.http_proxy", "http://alice@proxy.example"),
    ] {
        let error = local.set(key, ConfigInput::Plain(value));
        assert_eq!(error, Err(ConfigError::SecretRequired));
        assert!(!format!("{error:?} {local:?}").contains("credential@"));
    }
}

#[test]
fn safe_json_and_debug_never_expose_secret_values_or_handles() -> TestResult {
    let mut local = ConfigLayer::new();
    let handle = "test-vault-entry-42";
    let secret = "http://alice:never-print-this@proxy.example:7890";
    local.set(
        "env.https_proxy",
        ConfigInput::Secret {
            reference: SecretRef::new(handle)?,
            raw: secret,
        },
    )?;
    let config = resolve(&local)?;
    let json = serde_json::to_string(&config.list())?;
    let debug = format!("{local:?} {config:?}");
    for text in [&json, &debug] {
        for forbidden in [handle, secret, "alice", "never-print-this"] {
            assert!(!text.contains(forbidden));
        }
    }
    assert!(json.contains("\"redacted\":true"));
    assert!(json.contains("\"value\":null"));
    Ok(())
}

#[test]
fn secret_input_is_validated_before_a_reference_is_accepted() -> TestResult {
    let mut local = ConfigLayer::new();
    assert!(
        local
            .set(
                "env.https_proxy",
                ConfigInput::Secret {
                    reference: SecretRef::new("test-1")?,
                    raw: "http://host?secret=private",
                }
            )
            .is_err()
    );
    assert_eq!(
        local.set(
            "background",
            ConfigInput::Secret {
                reference: SecretRef::new("test-2")?,
                raw: "true",
            }
        ),
        Err(ConfigError::SecretNotAllowed)
    );
    Ok(())
}

#[test]
fn reserved_environment_cannot_redirect_identity_or_product_state() {
    for name in [
        "CODEX_HOME",
        "REMOTE_CODEX_TOKEN",
        "HOME",
        "XDG_RUNTIME_DIR",
        "remote_codex_context_id",
    ] {
        assert_eq!(
            EnvironmentName::parse(name),
            Err(ConfigError::ReservedEnvironment)
        );
    }
    for name in ["", "1INVALID", "KEY=VALUE", "BAD-NAME", "line\nbreak"] {
        assert_eq!(
            EnvironmentName::parse(name),
            Err(ConfigError::InvalidEnvironmentName)
        );
    }
}
