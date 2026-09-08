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
    assert!(project.resolve(home.path(), "rc_dev", None).is_err());
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
