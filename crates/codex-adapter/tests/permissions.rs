use remote_codex_adapter::{desktop, events};
use remote_codex_core::session::{ApprovalsReviewer, PermissionPreset, SessionSettings};
use serde_json::json;

#[test]
fn presets_set_both_execution_and_approval_policy() {
    for (permissions, reviewer, expected, approval, reviewed_by) in [
        (
            PermissionPreset::FullAccess,
            None,
            ":danger-full-access",
            "never",
            None,
        ),
        (
            PermissionPreset::Workspace,
            Some(ApprovalsReviewer::User),
            ":workspace",
            "on-request",
            Some("user"),
        ),
        (
            PermissionPreset::Workspace,
            Some(ApprovalsReviewer::AutoReview),
            ":workspace",
            "on-request",
            Some("auto_review"),
        ),
    ] {
        let params = events::settings(
            "thread",
            SessionSettings {
                permissions: Some(permissions),
                reviewer,
                ..Default::default()
            },
        );
        assert_eq!(params["permissions"], expected);
        assert_eq!(params["approvalPolicy"], approval);
        assert_eq!(params["approvalsReviewer"].as_str(), reviewed_by);
    }
    let params = events::settings(
        "thread",
        SessionSettings {
            model: Some("selected".into()),
            ..Default::default()
        },
    );
    assert!(params.get("approvalPolicy").is_none());
    assert!(params.get("permissions").is_none());
}

#[test]
fn confirmed_settings_keep_sandbox_and_approval_independent() {
    for (approval, expected) in [
        (json!("never"), "never"),
        (json!("on-request"), "on-request"),
        (json!({"reject":{"mcp_elicitations":true}}), "custom"),
        (json!(null), "custom"),
    ] {
        let confirmed = desktop::settings(
            &json!({"sandboxPolicy":{"type":"dangerFullAccess"}, "approvalPolicy":approval}),
        );
        assert!(confirmed.full_access);
        assert_eq!(confirmed.approval_policy, expected);
    }
}

#[tokio::test]
#[ignore = "requires REMOTE_CODEX_TEST_BINARY; temporary Codex home, no model turn"]
async fn native_permission_menu_loads_profiles_and_confirms_changes()
-> Result<(), Box<dyn std::error::Error>> {
    use remote_codex_adapter::{
        program::Launch,
        thread::{Codex, OpenSource, OpenThread, Prepared},
    };
    use remote_codex_core::session::SessionBinding;
    use std::{collections::BTreeMap, time::Duration};

    let program = Launch::new(std::env::var("REMOTE_CODEX_TEST_BINARY")?);
    let home = tempfile::tempdir()?;
    let cwd = home.path().to_str().ok_or("temporary path is not UTF-8")?;
    let codex = Codex::start(&program, home.path()).await?;
    let result = async {
        let opened = codex
            .open(OpenThread {
                environment: "local",
                directory: cwd,
                execution_mode: "sandboxed",
                source: OpenSource::New,
                mcp: BTreeMap::new(),
                instructions: String::new(),
            })
            .await?;
        let id = opened.session.id.clone();
        let binding = SessionBinding {
            server_id: "fixture".into(),
            remote_identity: "fixture".into(),
            environment_id: "local".into(),
            codex_home: cwd.into(),
            codex_version: "fixture".into(),
            execution_mode: "sandboxed".into(),
            revision: 1,
            session: opened.session.clone(),
        };
        let thread = codex.bind(opened, binding, false)?;
        let mappings = BTreeMap::new();
        let Prepared::Operation(request) = thread.prepare(
            "permissionProfile/list",
            json!({"cwd":cwd, "cursor":null, "limit":null}),
            false,
            &mappings,
        )?
        else {
            return Err("permission profiles were not forwarded".into());
        };
        let profiles = thread.execute(request).await?;
        let profiles = profiles["data"].as_array().ok_or("profiles missing")?;
        let mut notifications = thread.subscribe();
        for (preset, full_access, approval) in [
            (":danger-full-access", true, "never"),
            (":workspace", false, "on-request"),
        ] {
            assert!(
                profiles
                    .iter()
                    .any(|p| p["id"] == preset && p["allowed"] == true)
            );
            let Prepared::Operation(request) = thread.prepare(
                "thread/settings/update",
                json!({"threadId":id, "permissions":preset,
                    "approvalPolicy":approval, "approvalsReviewer":"user"}),
                false,
                &mappings,
            )?
            else {
                return Err("permission update was not forwarded".into());
            };
            thread.execute(request).await?;
            let confirmed = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let mut event = notifications.recv().await?;
                    if let events::Notice::Permissions {
                        thread,
                        full_access,
                    } = events::notice(&mut event, false)?
                        && thread == id
                    {
                        return Ok::<_, Box<dyn std::error::Error>>(full_access);
                    }
                }
            })
            .await??;
            assert_eq!(confirmed, full_access);
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    }
    .await;
    codex.shutdown().await;
    result
}
