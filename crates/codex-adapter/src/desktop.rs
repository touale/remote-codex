use crate::{engine::Engine, thread::Thread};
use remote_codex_core::desktop::{AccountStatus, ConfirmedSettings, McpStatus, ModelOption};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::path::Path;

pub struct NativeAccount {
    engine: Engine,
}
impl NativeAccount {
    pub async fn open(program: &Path, home: &Path) -> Result<Self, Fault> {
        Ok(Self {
            engine: Engine::local(program, home).await?.0,
        })
    }
    pub async fn status(&self) -> Result<AccountStatus, Fault> {
        let value = self
            .engine
            .call("account/read", json!({"refreshToken":false}))
            .await?;
        Ok(AccountStatus {
            logged_in: !value["account"].is_null() || value["requiresOpenaiAuth"] == false,
            email: value["account"]["email"].as_str().map(str::to_owned),
            plan: value["account"]["planType"].as_str().map(str::to_owned),
        })
    }
    pub async fn login(&self) -> Result<(String, String), Fault> {
        let value = self
            .engine
            .call("account/login/start", json!({"type":"chatgpt"}))
            .await?;
        Ok((required(&value, "loginId")?, required(&value, "authUrl")?))
    }
    pub async fn cancel(&self, id: &str) -> Result<(), Fault> {
        self.engine
            .call("account/login/cancel", json!({"loginId":id}))
            .await?;
        Ok(())
    }
    pub async fn usage(&self) -> Result<remote_codex_core::status::AccountUsage, Fault> {
        crate::usage::read(&self.engine).await
    }
    pub async fn defaults(
        &self,
        mode: &str,
    ) -> Result<remote_codex_core::desktop::SessionDefaults, Fault> {
        crate::defaults::read(&self.engine, mode).await
    }
    pub async fn close(&self) {
        self.engine.shutdown().await;
    }
}
impl Drop for NativeAccount {
    fn drop(&mut self) {
        self.engine.stop();
    }
}

impl Thread {
    pub async fn models(&self) -> Result<Vec<ModelOption>, Fault> {
        Ok(crate::defaults::catalog(&self.codex.engine).await?.models)
    }
    pub async fn mcp_status(&self) -> Result<Vec<McpStatus>, Fault> {
        let value = self
            .codex
            .engine
            .call(
                "mcpServerStatus/list",
                json!({"threadId":self.binding.session.id}),
            )
            .await?;
        Ok(value["data"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| McpStatus {
                name: text(v, "name"),
                status: v["runtimeStatus"].as_str().unwrap_or("unknown").into(),
                authentication: text(v, "authStatus"),
                tools: v["tools"].as_object().map_or(0, |v| v.len()),
            })
            .collect())
    }
}
pub fn settings(value: &Value) -> ConfirmedSettings {
    ConfirmedSettings {
        mode: if value["collaborationMode"]["mode"] == "plan" {
            remote_codex_core::goals::CollaborationMode::Plan
        } else {
            remote_codex_core::goals::CollaborationMode::Agent
        },
        model: text(value, "model"),
        effort: value["effort"].as_str().map(str::to_owned),
        full_access: value["sandboxPolicy"]["type"] == "dangerFullAccess",
        reviewer: value["approvalsReviewer"].as_str().unwrap_or("user").into(),
    }
}
pub use crate::tools::project as tool;
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().into()
}
fn required(value: &Value, key: &str) -> Result<String, Fault> {
    value[key].as_str().map(str::to_owned).ok_or_else(|| {
        Fault::new(
            "NATIVE_ACCOUNT_RESPONSE",
            "Codex returned an invalid login response.",
        )
    })
}
