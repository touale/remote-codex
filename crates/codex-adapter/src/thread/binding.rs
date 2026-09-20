use remote_codex_core::session::{Session, SessionBinding};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};

pub(crate) const START_APPROVAL_POLICY: &str = "on-request";

pub(crate) fn start_params(environment: &str, cwd: &str, mode: &str) -> Value {
    json!({"cwd":cwd,"environments":[{"environmentId":environment,"cwd":cwd}],
        "approvalPolicy":START_APPROVAL_POLICY,"sandbox":if full_access(mode) {"danger-full-access"} else {"workspace-write"},
        "config":{"features.multi_agent":false},"experimentalRawEvents":false})
}

pub(crate) fn turn_params(params: &mut Value, binding: &SessionBinding) {
    params["cwd"] = json!(binding.session.cwd);
    params["environments"] =
        json!([{"environmentId":binding.environment_id,"cwd":binding.session.cwd}]);
    params["runtimeWorkspaceRoots"] = json!([binding.session.cwd]);
}

pub(crate) fn session(thread: &Value, cwd: &str) -> Result<Session, Fault> {
    Ok(Session {
        id: thread["id"]
            .as_str()
            .ok_or(Fault::new(
                "INVALID_NATIVE_THREAD",
                "native thread identity is missing",
            ))?
            .into(),
        title: thread["name"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| preview(thread["preview"].as_str().unwrap_or_default())),
        cwd: cwd.into(),
        created_at: thread["createdAt"].as_i64().unwrap_or(0),
        updated_at: thread["updatedAt"].as_i64().unwrap_or(0),
        archived: thread["archived"].as_bool().unwrap_or(false),
        state: thread
            .pointer("/status/type")
            .and_then(Value::as_str)
            .unwrap_or("idle")
            .into(),
    })
}

pub(crate) fn full_access(mode: &str) -> bool {
    mode == "unrestricted"
}

fn preview(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return "New session".into();
    }
    if text.chars().count() > 80 {
        format!("{}…", text.chars().take(79).collect::<String>())
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_uses_a_nonempty_name_then_a_compact_unicode_preview() -> Result<(), Fault> {
        let mut thread = json!({"id":"test","name":" ","preview":"  Plan\n this  task  "});
        assert_eq!(session(&thread, "/")?.title, "Plan this task");
        thread["name"] = json!("My title");
        assert_eq!(session(&thread, "/")?.title, "My title");
        assert_eq!(preview("\n"), "New session");
        assert_eq!(preview(&"目".repeat(90)), format!("{}…", "目".repeat(79)));
        Ok(())
    }
}
