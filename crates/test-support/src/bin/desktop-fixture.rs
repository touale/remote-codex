//! Loopback model sidecar for native desktop acceptance. No live model credentials.
use remote_codex_test_support::{
    ProbeResult,
    model::{ModelFixture, function, message},
};
use serde_json::json;
use std::{io::Write, path::PathBuf};
#[tokio::main]
async fn main() -> ProbeResult<()> {
    let home = PathBuf::from(std::env::var_os("CODEX_HOME").ok_or("isolated Codex home required")?);
    let workspace = std::env::var("REMOTE_CODEX_E2E_WORKSPACE")?;
    let model = ModelFixture::start().await?;
    std::fs::create_dir_all(&home)?;
    let catalog = home.join("models.json");
    std::fs::write(
        &catalog,
        serde_json::to_vec(&json!({"models":[{
            "slug":"gpt-5.4-mini", "display_name":"gpt-5.4-mini", "description":"Loopback test model",
            "default_reasoning_level":"medium", "supported_reasoning_levels":[{"effort":"medium","description":"Fixture reasoning"}],
            "shell_type":"unified_exec", "visibility":"list", "supported_in_api":true, "priority":0,
            "base_instructions":"You are a coding assistant.", "experimental_supported_tools":["send_user_message_async"],
            "support_verbosity":true, "context_window":200000, "truncation_policy":{"mode":"tokens","limit":10000}
        }]}))?,
    )?;
    std::fs::write(
        home.join("config.toml"),
        format!(
            "model_catalog_json={}\nmodel=\"fixture-model\"\nmodel_provider=\"fixture\"\n[model_providers.fixture]\nname=\"Desktop acceptance\"\nbase_url={}\nwire_api=\"responses\"\nrequires_openai_auth=false\n[analytics]\nenabled=false\n",
            serde_json::to_string(&catalog)?,
            serde_json::to_string(&model.base_url)?
        ),
    )?;
    model.script([
        message("The remote workspace is ready. Files, commands and project tools stay on the server."),
        function("exec_command", json!({"cmd":"printf 'from-codex\n' > approval.txt", "workdir":workspace, "sandbox_permissions":"require_escalated", "justification":"Write the desktop acceptance marker"}), None),
        message("Created approval.txt in the remote workspace."),
        message("<proposed_plan>\n# Workspace plan\nInspect the remote workspace and report the result.\n</proposed_plan>"),
        message("<proposed_plan>\n# Revised workspace plan\nInspect the workspace, include a verification step, and report the result.\n</proposed_plan>"),
        message("Implemented the revised plan in a fresh context."),
        message("Implemented the selected workspace plan."),
        function("exec_command",json!({"cmd":"printf 'goal-ok\\n' > goal.txt","workdir":workspace,"sandbox_permissions":"require_escalated","justification":"Write the isolated Goal acceptance marker"}),None),
        function("update_goal",json!({"status":"complete"}),None),
        message("The workspace goal is complete."),
        message("<proposed_plan>\nA plan from a fresh draft.\n</proposed_plan>"),
        function("update_goal",json!({"status":"complete"}),None),
        message("A fresh goal is complete."),
        function("request_user_input_async", json!({"questions":[
            {"title":"Which scope?","options":["Research","Implementation"]},
            {"title":"Which budget?","options":["Small","Large"]}
        ]}), None),
        message("Waiting for your choices."),
        message("The choices are recorded."),
        message("The revised question has been processed."),
        message("The first message was regenerated."),
    ])?;
    println!("ready");
    std::io::stdout().flush()?;
    let _ = tokio::signal::ctrl_c().await;
    if let Some(path) = std::env::var_os("REMOTE_CODEX_E2E_REQUESTS") {
        std::fs::write(path, serde_json::to_vec_pretty(&model.requests()?)?)?;
    }
    Ok(())
}
