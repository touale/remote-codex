use super::*;

#[test]
fn local_mcp_is_pinned_locally_and_conflicts_require_source_choice() -> Result<()> {
    let home = tempfile::tempdir()?;
    std::fs::write(
        home.path().join("config.toml"),
        "[mcp_servers.shared]\ncommand=\"local-command\"\n[mcp_servers.shared.env]\nSECRET=\"local-only\"\n",
    )?;
    let mut project = ProjectMcp {
        path: "/workspace".into(),
        names: vec![],
        trusted: true,
        digest: String::new(),
        servers: BTreeMap::new(),
    };
    let local = project.resolve(home.path(), "rc_dev", None)?;
    assert_eq!(
        local.config["mcp_servers.shared"]["environment_id"],
        "local"
    );
    assert!(local.commands.is_empty());
    project.names.push("shared".into());
    project
        .servers
        .insert("shared".into(), json!({"command":"remote-command"}));
    let error = project
        .resolve(home.path(), "rc_dev", None)
        .err()
        .ok_or(ClientError::RemoteResponse)?;
    assert_eq!(error.code(), "MCP_NAME_CONFLICT");
    assert_eq!(error.exit_code(), 2);
    let preferred = project.resolve(home.path(), "rc_dev", Some("local"))?;
    assert!(preferred.commands.is_empty());
    assert_eq!(
        preferred.config["mcp_servers.shared"]["command"],
        "local-command"
    );
    let remote = project.resolve(home.path(), "rc_dev", Some("remote"))?;
    assert_eq!(
        remote.config["mcp_servers.shared"]["environment_id"],
        "rc_dev"
    );
    assert!(!serde_json::to_string(&remote.config)?.contains("local-only"));
    project.trusted = false;
    assert!(
        project
            .resolve(home.path(), "rc_dev", Some("remote"))
            .is_err()
    );
    Ok(())
}

#[test]
fn recovery_resolves_current_local_mcp_without_changing_project_trust() -> Result<()> {
    let home = tempfile::tempdir()?;
    let config = home.path().join("config.toml");
    let project = ProjectMcp {
        path: "/workspace".into(),
        names: vec!["remote".into()],
        trusted: true,
        digest: "reviewed-project".into(),
        servers: BTreeMap::from([("remote".into(), json!({"command":"remote-command"}))]),
    };
    for (text, expected) in [
        (
            "[mcp_servers.local]\ncommand='before'\n",
            Some(("before", true)),
        ),
        (
            "[mcp_servers.local]\ncommand='after'\n",
            Some(("after", true)),
        ),
        (
            "[mcp_servers.local]\ncommand='after'\nenabled=false\n",
            Some(("after", false)),
        ),
        ("", None),
    ] {
        std::fs::write(&config, text)?;
        let resolved = project.resolve(home.path(), "rc_dev", None)?;
        let actual = resolved.config.get("mcp_servers.local").map(|settings| {
            (
                settings["command"].as_str().unwrap_or_default(),
                settings["enabled"] != false,
            )
        });
        assert_eq!(actual, expected);
        assert_eq!(resolved.commands.len(), 1);
        assert_eq!(resolved.commands[0].argv, ["remote-command"]);
        assert_eq!(
            resolved.config["mcp_servers.remote"]["environment_id"],
            "rc_dev"
        );
    }
    for text in [
        "[mcp_servers",
        "mcp_servers=1",
        "[mcp_servers.local]\ncommand='local-command'\nenvironment_id='another-server'\n",
    ] {
        std::fs::write(&config, text)?;
        assert!(
            project
                .resolve(home.path(), "rc_dev", None)
                .err()
                .is_some_and(|error| error.code() == "MCP_CONFIGURATION_INVALID")
        );
    }
    Ok(())
}
