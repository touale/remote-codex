use super::{Context, environment::quote, headless};
use remote_codex_client::{
    application::OpenSession,
    session::{PermissionPreset, SessionSettings},
};
use remote_codex_test_support::{ProbeResult, model::function};
use serde_json::{Value, json};
use std::io::Write;

pub(super) async fn exercise(context: &Context) -> ProbeResult<()> {
    let environment = &context.environment;
    let skill = environment.home.join("skills/staged-probe");
    std::fs::create_dir_all(&skill)?;
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: staged-probe\ndescription: Verify staged remote skill resources.\n---\nRun verify.sh in this directory.\n",
    )?;
    std::fs::write(skill.join("verify.sh"), "printf STAGED_SKILL_RESOURCE\n")?;
    let local_mcp = environment.home.join("mcp.sh");
    std::fs::write(&local_mcp, include_str!("../../fixtures/mcp.sh"))?;
    let remote_mcp = format!("{}/mcp.sh", environment.workspace);
    let project = format!("{}/.codex/config.toml", environment.workspace);
    environment
        .write(&remote_mcp, include_str!("../../fixtures/mcp.sh"))
        .await?;
    let project_config = format!(
        "[mcp_servers.remote_probe]\ncommand=\"/bin/sh\"\nargs=[{}]\n[mcp_servers.remote_probe.env]\nPROBE_MARKER=\"REMOTE_MCP\"\n",
        serde_json::to_string(&remote_mcp)?
    );
    environment.write(&project, &project_config).await?;
    let local_config = format!(
        "\n[mcp_servers.local_probe]\ncommand=\"/bin/sh\"\nargs=[{}]\nrequired=true\n[mcp_servers.local_probe.env]\nPROBE_MARKER=\"LOCAL_MCP\"\n",
        serde_json::to_string(&local_mcp)?
    );
    std::fs::OpenOptions::new()
        .append(true)
        .open(environment.home.join("config.toml"))?
        .write_all(local_config.as_bytes())?;
    let options = OpenSession {
        server: "test".into(),
        path: environment.workspace.clone(),
        resume: None,
        takeover: false,
        mcp_source: None,
    };
    let prepared = context.client.sessions().prepare(options.clone()).await?;
    if !prepared.trust().required {
        return Err("project MCP was trusted without consent".into());
    }
    let session = prepared.open(true).await?;
    session
        .settings(SessionSettings {
            permissions: Some(PermissionPreset::FullAccess),
            ..Default::default()
        })
        .await?;
    headless::turn(&session, false).await?;
    let requests = context.model.requests()?;
    let mut text = String::new();
    strings(requests.last().ok_or("model request missing")?, &mut text);
    if !text.contains("LOCAL_INSTRUCTIONS_MARKER") {
        return Err("local developer instructions were lost".into());
    }
    let remote = text
        .lines()
        .find_map(|line| {
            let (local, remote) = line.split_once(" => ")?;
            local
                .contains("staged-probe/SKILL.md")
                .then(|| serde_json::from_str::<String>(remote).ok())
                .flatten()
        })
        .ok_or("staged skill mapping was not sent to the model")?;
    let directory = std::path::Path::new(&remote)
        .parent()
        .and_then(|path| path.to_str())
        .ok_or("invalid skill mapping")?;
    context.model.script([
        function("exec_command", json!({"cmd":format!("sh {}/verify.sh", quote(directory)?), "workdir":environment.workspace}), None),
        function("probe_identity", json!({}), Some("mcp__remote_probe")),
        function("probe_identity", json!({}), Some("mcp__local_probe")),
    ])?;
    headless::turn(&session, false).await?;
    let request = context
        .model
        .requests()?
        .last()
        .ok_or("model result missing")?
        .to_string();
    for marker in [
        "STAGED_SKILL_RESOURCE",
        "OS=Linux MARKER=REMOTE_MCP",
        "OS=Darwin MARKER=LOCAL_MCP",
    ] {
        if !request.contains(marker) {
            return Err(format!("extension result missing: {marker}").into());
        }
    }
    session.close().await;
    let prepared = context.client.sessions().prepare(options.clone()).await?;
    if prepared.trust().required {
        return Err("unchanged project MCP trust was not persisted".into());
    }
    drop(prepared);
    environment
        .write(
            &project,
            &project_config.replace("REMOTE_MCP", "CHANGED_MCP"),
        )
        .await?;
    let changed = context.client.sessions().prepare(options).await?;
    if !changed.trust().required {
        return Err("changed project MCP did not require renewed trust".into());
    }
    drop(changed);
    environment.write(&project, &project_config).await?;
    eprintln!("extensions: Skills, local/remote MCP and digest-scoped trust passed");
    Ok(())
}

fn strings(value: &Value, result: &mut String) {
    match value {
        Value::String(text) => {
            result.push_str(text);
            result.push('\n');
        }
        Value::Array(values) => {
            for value in values {
                strings(value, result);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                strings(value, result);
            }
        }
        _ => {}
    }
}
