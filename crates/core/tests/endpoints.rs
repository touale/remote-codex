use remote_codex_core::connection::{SshEndpoint, validate_name};

#[test]
fn preserves_ssh_alias_overrides_and_normalizes_ip_literals()
-> Result<(), Box<dyn std::error::Error>> {
    let alias = SshEndpoint::parse("Dev-Host", None)?;
    assert_eq!(alias.host(), "Dev-Host");
    assert_eq!(alias.port(), None);
    let ipv6 = SshEndpoint::parse("root@[2001:0db8::1]", Some(43256))?;
    assert_eq!(ipv6.default_name(), "root@[2001:db8::1]:43256");
    assert_eq!(SshEndpoint::parse("root@2001:db8::1", Some(43256))?, ipv6);
    assert_eq!(
        SshEndpoint::parse("root@127.0.0.1", Some(43256))?.port(),
        Some(43256)
    );
    Ok(())
}

#[test]
fn endpoint_cannot_inject_ssh_options_or_shell_syntax() {
    for target in [
        "",
        "-oProxyCommand=evil",
        "host;id",
        "user@host\n",
        "user@@host",
        "$(id)",
        "[bad]",
        "root@host:22",
        "-root@host",
    ] {
        assert!(
            SshEndpoint::parse(target, None).is_err(),
            "accepted unsafe target"
        );
    }
    assert!(SshEndpoint::parse("host", Some(0)).is_err());
    assert!(validate_name("-bad").is_err());
    assert!(validate_name("name\n").is_err());
}
