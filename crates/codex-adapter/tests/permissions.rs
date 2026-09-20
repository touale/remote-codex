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
