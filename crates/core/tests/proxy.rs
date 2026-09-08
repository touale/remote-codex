use std::collections::BTreeMap;

use remote_codex_core::config::{
    ConfigError, ConfigInput, ConfigKey, ConfigLayer, ConfigValue, plan_environment, resolve,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn conflicting_aliases_are_rejected_within_one_server() -> TestResult {
    let mut layer = ConfigLayer::new();
    layer.set(
        "env.https_proxy",
        ConfigInput::Plain("http://one.example:7890"),
    )?;
    layer.set(
        "env.HTTPS_PROXY",
        ConfigInput::Plain("http://one.example:7890"),
    )?;
    assert_eq!(
        layer.set(
            "env.HTTPS_PROXY",
            ConfigInput::Plain("http://two.example:7890")
        ),
        Err(ConfigError::ConflictingProxyAliases)
    );
    layer.set(
        "env.https_proxy",
        ConfigInput::Plain("http://two.example:7890"),
    )?;
    assert!(layer.unset("env.HTTPS_PROXY")?);
    layer.set(
        "env.HTTPS_PROXY",
        ConfigInput::Plain("http://three.example:7890"),
    )?;
    Ok(())
}

#[test]
fn inherit_uses_remote_proxies_and_overrides_both_aliases() -> TestResult {
    let mut local = ConfigLayer::new();
    local.set("proxy.mode", ConfigInput::Plain("inherit"))?;
    local.set(
        "env.https_proxy",
        ConfigInput::Plain("http://127.0.0.1:7890"),
    )?;
    local.set("env.LANG", ConfigInput::Plain("zh_CN.UTF-8"))?;
    let remote = BTreeMap::from([
        ("HTTPS_PROXY".to_owned(), "http://old.example:80".to_owned()),
        (
            "https_proxy".to_owned(),
            "http://other.example:80".to_owned(),
        ),
        (
            "HTTP_PROXY".to_owned(),
            "http://remote.example:80".to_owned(),
        ),
        ("SECRET_DO_NOT_COPY".to_owned(), "private".to_owned()),
    ]);
    let plan = plan_environment(&resolve(&local)?, &remote)?;
    assert_eq!(
        plan.set.get("HTTPS_PROXY"),
        Some(&ConfigValue::Text("http://127.0.0.1:7890".to_owned()))
    );
    assert_eq!(plan.set.get("https_proxy"), plan.set.get("HTTPS_PROXY"));
    assert_eq!(
        plan.set.get("http_proxy"),
        Some(&ConfigValue::Text("http://remote.example:80".to_owned()))
    );
    assert!(plan.set.contains_key("LANG"));
    assert!(!plan.set.contains_key("SECRET_DO_NOT_COPY"));
    Ok(())
}

#[test]
fn direct_clears_all_proxy_aliases_and_reports_masked_settings() -> TestResult {
    let mut local = ConfigLayer::new();
    local.set(
        "env.https_proxy",
        ConfigInput::Plain("http://configured.example:7890"),
    )?;
    local.set("proxy.mode", ConfigInput::Plain("direct"))?;
    local.set("env.no_proxy", ConfigInput::Plain("localhost"))?;
    local.set("env.LANG", ConfigInput::Plain("C"))?;
    let config = resolve(&local)?;
    let remote = BTreeMap::from([(
        "Https_Proxy".to_owned(),
        "invalid inherited value".to_owned(),
    )]);
    let plan = plan_environment(&config, &remote)?;
    assert_eq!(plan.set.len(), 1);
    for name in [
        "http_proxy",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "Https_Proxy",
    ] {
        assert!(plan.remove.contains(name));
    }
    assert_eq!(
        config
            .view(&ConfigKey::parse("env.https_proxy")?)
            .map(|v| v.masked_by_proxy_mode),
        Some(true)
    );
    Ok(())
}

#[test]
fn custom_requires_an_explicit_address_and_never_inherits_remote_proxies() -> TestResult {
    let mut local = ConfigLayer::new();
    local.set("proxy.mode", ConfigInput::Plain("custom"))?;
    local.set("env.no_proxy", ConfigInput::Plain(""))?;
    assert!(matches!(
        resolve(&local),
        Err(ConfigError::MissingCustomProxy)
    ));
    local.set("env.all_proxy", ConfigInput::Plain("socks5h://[::1]:1080"))?;
    let remote = BTreeMap::from([("HTTPS_PROXY".to_owned(), "invalid".to_owned())]);
    let plan = plan_environment(&resolve(&local)?, &remote)?;
    assert!(!plan.set.contains_key("HTTPS_PROXY"));
    assert!(plan.set.contains_key("ALL_PROXY"));
    Ok(())
}

#[test]
fn conflicting_inherited_proxies_are_visible_errors() -> TestResult {
    let mut local = ConfigLayer::new();
    local.set("proxy.mode", ConfigInput::Plain("inherit"))?;
    let config = resolve(&local)?;
    let remote = BTreeMap::from([
        ("https_proxy".to_owned(), "http://one.example:80".to_owned()),
        ("HTTPS_PROXY".to_owned(), "http://two.example:80".to_owned()),
    ]);
    assert!(matches!(
        plan_environment(&config, &remote),
        Err(ConfigError::ConflictingProxyAliases)
    ));
    Ok(())
}

#[test]
fn proxy_url_validation_rejects_silent_normalization_and_hidden_fields() {
    let mut local = ConfigLayer::new();
    for invalid in [
        "",
        "127.0.0.1:7890",
        "file:///tmp/proxy",
        "http://host:0",
        "http://host/path",
        "http://host?token=x",
        "http://host#fragment",
        "http://host\n",
        "http://host\\evil",
        " http://host",
        "http://",
    ] {
        assert!(
            local
                .set("env.https_proxy", ConfigInput::Plain(invalid))
                .is_err(),
            "accepted invalid proxy syntax"
        );
    }
}
