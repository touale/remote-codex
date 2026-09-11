//! Account usage reads shared by local account views and bound threads.
use crate::engine::Engine;
use remote_codex_core::status::{AccountUsage, UsageAvailability};
use remote_codex_protocol::Fault;
use serde_json::json;

pub(crate) async fn read(engine: &Engine) -> Result<AccountUsage, Fault> {
    tokio::time::timeout(std::time::Duration::from_secs(8), async {
        let account = engine
            .call("account/read", json!({"refreshToken":false}))
            .await?;
        let availability = if account["account"]["type"] == "apiKey"
            || (account["account"].is_null() && account["requiresOpenaiAuth"] == false)
        {
            UsageAvailability::Unsupported
        } else if account["account"].is_null() {
            UsageAvailability::SignedOut
        } else {
            UsageAvailability::Available
        };
        if !matches!(availability, UsageAvailability::Available) {
            return Ok(AccountUsage {
                account_id: None,
                availability,
                limits: Vec::new(),
            });
        }
        let value = engine.call("account/rateLimits/read", json!({})).await?;
        Ok(AccountUsage {
            account_id: value["accountId"].as_str().map(str::to_owned),
            availability,
            limits: crate::status::limits(&value),
        })
    })
    .await
    .map_err(|_| {
        Fault::new(
            "STATUS_TIMEOUT",
            "Usage limits are temporarily unavailable.",
        )
    })?
}
