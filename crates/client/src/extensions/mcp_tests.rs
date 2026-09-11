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
