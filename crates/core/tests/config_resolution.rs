use remote_codex_core::config::{
    ConfigError, ConfigInput, ConfigKey, ConfigLayer, ConfigValue, ProxyMode, Source, resolve,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn server_settings_preserve_false_zero_and_empty_values() -> TestResult {
    let mut server = ConfigLayer::new();
    server.set("background", ConfigInput::Plain("false"))?;
    server.set("disconnect_grace_seconds", ConfigInput::Plain("0"))?;
    server.set("env.LANGUAGE", ConfigInput::Plain(""))?;
    let config = resolve(&server)?;
    assert_eq!(
        config.get(&ConfigKey::Background),
        Some(&ConfigValue::Boolean(false))
    );
    assert_eq!(
        config.get(&ConfigKey::DisconnectGraceSeconds),
        Some(&ConfigValue::Integer(0))
    );
    assert_eq!(
        config.get(&ConfigKey::parse("env.LANGUAGE")?),
        Some(&ConfigValue::Text(String::new()))
    );
    assert_eq!(
        config.view(&ConfigKey::Background).map(|item| item.source),
        Some(Source::Server)
    );
    Ok(())
}

#[test]
fn servers_have_independent_settings_and_start_with_direct_networking() -> TestResult {
    let mut first = ConfigLayer::new();
    first.set("proxy.mode", ConfigInput::Plain("custom"))?;
    first.set(
        "env.HTTPS_PROXY",
        ConfigInput::Plain("http://127.0.0.1:10809"),
    )?;
    first.set("env.no_proxy", ConfigInput::Plain("localhost"))?;
    let one = resolve(&first)?;
    let two = resolve(&ConfigLayer::new())?;
    let proxy = ConfigKey::parse("env.https_proxy")?;
    assert!(one.get(&proxy).is_some());
    assert!(two.get(&proxy).is_none());
    assert!(two.get(&ConfigKey::parse("env.no_proxy")?).is_none());
    assert_eq!(two.proxy_mode(), ProxyMode::Direct);
    assert_eq!(
        two.get(&ConfigKey::Background),
        Some(&ConfigValue::Boolean(true))
    );
    assert_eq!(
        two.get(&ConfigKey::DisconnectGraceSeconds),
        Some(&ConfigValue::Integer(30))
    );
    assert!(two.list().iter().all(|item| item.source == Source::Default));
    Ok(())
}

#[test]
fn unset_restores_builtin_default_and_does_not_change_existing_snapshots() -> TestResult {
    let mut server = ConfigLayer::new();
    server.set("background", ConfigInput::Plain("false"))?;
    let captured = resolve(&server)?;
    assert!(server.unset("background")?);
    let defaults = resolve(&server)?;
    assert_eq!(
        captured.get(&ConfigKey::Background),
        Some(&ConfigValue::Boolean(false))
    );
    assert_eq!(
        defaults.get(&ConfigKey::Background),
        Some(&ConfigValue::Boolean(true))
    );
    assert_eq!(
        defaults
            .view(&ConfigKey::Background)
            .map(|item| item.source),
        Some(Source::Default)
    );
    assert!(!server.unset("background")?);
    Ok(())
}

#[test]
fn readonly_and_unknown_keys_cannot_mutate_configuration() -> TestResult {
    let mut server = ConfigLayer::new();
    for key in ["workspace", "bakground"] {
        assert_eq!(
            server.set(key, ConfigInput::Plain("value")),
            Err(ConfigError::UnknownKey)
        );
    }
    for key in ["ssh.host", "ssh.user", "ssh.port"] {
        assert_eq!(
            server.set(key, ConfigInput::Plain("example")),
            Err(ConfigError::ReadOnlySetting)
        );
        assert_eq!(server.unset(key), Err(ConfigError::ReadOnlySetting));
    }
    assert_eq!(resolve(&server)?.list().len(), 4);
    server.set("execution.mode", ConfigInput::Plain("unrestricted"))?;
    Ok(())
}

#[test]
fn invalid_values_leave_previous_configuration_intact() -> TestResult {
    let mut server = ConfigLayer::new();
    server.set("disconnect_grace_seconds", ConfigInput::Plain("0"))?;
    for invalid in ["-1", "+1", "3601", "1.0", "", " 30", "9999999"] {
        assert!(
            server
                .set("disconnect_grace_seconds", ConfigInput::Plain(invalid))
                .is_err()
        );
    }
    assert_eq!(
        server.get(&ConfigKey::DisconnectGraceSeconds),
        Some(&ConfigValue::Integer(0))
    );
    server.set("execution.mode", ConfigInput::Plain("sandboxed"))?;
    assert!(
        server
            .set("background", ConfigInput::Plain("FALSE"))
            .is_err()
    );
    Ok(())
}
