use super::{Thread, execution_policy, settings};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::time::Duration;

impl Thread {
    pub async fn restore_settings(
        &mut self,
        snapshot: &Value,
        full_access: bool,
    ) -> Result<bool, Fault> {
        let params = settings::restore(snapshot, &self.binding, full_access)?;
        let mut events = self.subscribe();
        self.codex
            .engine
            .call("thread/settings/update", params.clone())
            .await?;
        // Native acknowledges queued updates, but emits no notification for a no-op.
        // A response alone must never authorize execution in the replacement channel.
        let notification = tokio::time::timeout(Duration::from_secs(5), async {
            while let Ok(event) = events.recv().await {
                if event["method"] == "remoteCodex/engineClosed" {
                    break;
                }
                if event["method"] == "thread/settings/updated"
                    && event["params"]["threadId"] == self.binding.session.id
                    && mismatched_setting(
                        &event["params"]["threadSettings"],
                        &params,
                        &snapshot["sandboxPolicy"],
                        &self.binding.session.cwd,
                    )
                    .is_none()
                {
                    return Some(event["params"]["threadSettings"].clone());
                }
            }
            None
        })
        .await;
        let confirmed = match notification {
            Ok(Some(settings)) => settings,
            _ => {
                // This thread is already loaded and its goal is paused. Resume without
                // overrides reads live settings; it neither replays work nor loads turns.
                let mut current = self
                    .codex
                    .engine
                    .call(
                        "thread/resume",
                        json!({"threadId":self.binding.session.id,"excludeTurns":true}),
                    )
                    .await
                    .map_err(|error| unconfirmed(&error.to_string()))?;
                if current["thread"]["id"] != self.binding.session.id {
                    return Err(Fault::new(
                        "SESSION_MISMATCH",
                        "recovery returned another thread",
                    ));
                }
                current["sandboxPolicy"] = current["sandbox"].clone();
                current["effort"] = current["reasoningEffort"].clone();
                if let Some(field) = mismatched_setting(
                    &current,
                    &params,
                    &snapshot["sandboxPolicy"],
                    &self.binding.session.cwd,
                ) {
                    return Err(unconfirmed(&format!(
                        "native {field} does not match the recovery snapshot"
                    )));
                }
                current
            }
        };
        execution_policy::validate(&confirmed, &self.binding, full_access)?;
        if let Some(settings) = confirmed.as_object() {
            for (key, value) in settings {
                self.bootstrap[key] = value.clone();
            }
        }
        self.bootstrap["sandbox"] = confirmed["sandboxPolicy"].clone();
        self.bootstrap["reasoningEffort"] = confirmed["effort"].clone();
        Ok(confirmed["sandboxPolicy"]["type"] == "dangerFullAccess")
    }
}

fn mismatched_setting(
    current: &Value,
    requested: &Value,
    sandbox: &Value,
    cwd: &str,
) -> Option<&'static str> {
    if current["cwd"] != cwd {
        return Some("cwd");
    }
    if current.get("sandboxPolicy") != Some(sandbox) {
        return Some("sandboxPolicy");
    }
    [
        "model",
        "effort",
        "serviceTier",
        "approvalPolicy",
        "approvalsReviewer",
        "disabledPluginIds",
    ]
    .into_iter()
    .find(|key| {
        requested.get(*key).is_some_and(|value| {
            let actual = &current[*key];
            // Clearing the tier can be reported as the explicit native "default" tier.
            match (*key, actual, value) {
                ("serviceTier", Value::Null, Value::String(tier))
                | ("serviceTier", Value::String(tier), Value::Null)
                    if tier == "default" =>
                {
                    false
                }
                _ => actual != value,
            }
        })
    })
}

fn unconfirmed(detail: &str) -> Fault {
    Fault::new(
        "RECOVERY_SETTINGS_UNCONFIRMED",
        &format!("native settings could not be verified after recovery: {detail}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_requires_matching_live_permissions_and_preferences() {
        let sandbox =
            json!({"type":"workspaceWrite","writableRoots":["/workspace"],"networkAccess":false});
        let requested = json!({"model":"test-model","effort":"low","serviceTier":null,
            "approvalPolicy":"on-request","approvalsReviewer":"user","disabledPluginIds":["fixture.plugin"]});
        let mut current = requested.clone();
        current["cwd"] = json!("/workspace");
        current["sandboxPolicy"] = sandbox.clone();
        assert_eq!(
            mismatched_setting(&current, &requested, &sandbox, "/workspace"),
            None
        );
        current["serviceTier"] = json!("default");
        assert_eq!(
            mismatched_setting(&current, &requested, &sandbox, "/workspace"),
            None
        );
        for (key, value) in [
            ("cwd", json!("/other")),
            ("model", json!("other-model")),
            ("effort", json!("high")),
            ("serviceTier", json!("fast")),
            ("approvalPolicy", json!("never")),
            ("approvalsReviewer", json!("auto_review")),
            ("disabledPluginIds", json!([])),
            ("sandboxPolicy", json!({"type":"dangerFullAccess"})),
            (
                "sandboxPolicy",
                json!({"type":"workspaceWrite","writableRoots":["/"],"networkAccess":false}),
            ),
        ] {
            let mut different = current.clone();
            different[key] = value;
            assert_eq!(
                mismatched_setting(&different, &requested, &sandbox, "/workspace"),
                Some(key)
            );
        }
        assert_eq!(
            mismatched_setting(&json!({}), &requested, &sandbox, "/workspace"),
            Some("cwd")
        );
    }
}
