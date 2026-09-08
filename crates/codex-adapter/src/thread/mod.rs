pub mod action;
mod binding;
mod execution_policy;
pub mod history;
mod request;
mod settings;

use crate::engine::Engine;
use remote_codex_core::session::{Session, SessionBinding};
use remote_codex_protocol::Fault;
pub use request::{Operation, OperationKind, Prepared, response};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Codex {
    pub(crate) engine: Engine,
    initialized: Value,
}

pub struct OpenThread<'a> {
    pub environment: &'a str,
    pub directory: &'a str,
    pub execution_mode: &'a str,
    pub existing: Option<&'a str>,
    pub mcp: BTreeMap<String, Value>,
    pub instructions: String,
}

pub struct Opened {
    pub session: Session,
    pub full_access: bool,
    response: Value,
}

pub struct Thread {
    codex: Codex,
    binding: SessionBinding,
    bootstrap: Value,
}

impl Codex {
    pub async fn start(program: &Path, home: &Path) -> Result<Self, Fault> {
        let (engine, initialized) = Engine::local(program, home).await?;
        Ok(Self {
            engine,
            initialized,
        })
    }

    pub async fn register_environment(&self, id: &str, url: &str) -> Result<(), Fault> {
        self.engine
            .call(
                "environment/add",
                json!({"environmentId":id,"execServerUrl":url,"connectTimeoutMs":15000}),
            )
            .await?;
        Ok(())
    }

    pub async fn enabled_skills(&self, home: &Path) -> Result<Vec<PathBuf>, Fault> {
        let result = self
            .engine
            .call("skills/list", json!({"cwds":[home],"forceReload":true}))
            .await?;
        Ok(result["data"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|group| group["skills"].as_array().into_iter().flatten())
            .filter(|skill| skill["enabled"] != false)
            .filter_map(|skill| skill["path"].as_str().map(PathBuf::from))
            .collect())
    }

    pub async fn open(&self, options: OpenThread<'_>) -> Result<Opened, Fault> {
        let mut params = binding::start_params(
            options.environment,
            options.directory,
            options.execution_mode,
        );
        for (key, value) in options.mcp {
            params["config"][key] = value;
        }
        if !options.instructions.is_empty() {
            let native = self
                .engine
                .call("config/read", json!({"includeLayers":false}))
                .await?;
            let inherited = native["config"]["developer_instructions"]
                .as_str()
                .unwrap_or_default();
            params["developerInstructions"] =
                json!(format!("{inherited}\n\n{}", options.instructions));
        }
        params["dynamicTools"] = crate::recovery::tools();
        let response = if let Some(id) = options.existing {
            if let Some(object) = params.as_object_mut() {
                for key in ["environments", "sandbox", "approvalPolicy"] {
                    object.remove(key);
                }
            }
            params["threadId"] = json!(id);
            self.engine.call("thread/resume", params).await?
        } else {
            self.engine.call("thread/start", params).await?
        };
        Ok(Opened {
            session: binding::session(&response["thread"], options.directory)?,
            full_access: response.pointer("/sandbox/type").and_then(Value::as_str)
                == Some("dangerFullAccess"),
            response,
        })
    }

    pub fn bind(
        &self,
        opened: Opened,
        binding: SessionBinding,
        restoring: bool,
    ) -> Result<Thread, Fault> {
        execution_policy::validate(
            &json!({"cwd":opened.response["cwd"],"sandboxPolicy":opened.response["sandbox"],"approvalsReviewer":opened.response["approvalsReviewer"]}),
            &binding,
            restoring,
        )?;
        Ok(Thread {
            codex: self.clone(),
            binding,
            bootstrap: opened.response,
        })
    }

    pub async fn shutdown(&self) {
        self.engine.shutdown().await;
    }
    pub fn stop(&self) {
        self.engine.stop();
    }
}

impl Thread {
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Value> {
        self.codex.engine.subscribe()
    }
    pub fn send(&self, message: Value) -> Result<(), Fault> {
        self.codex.engine.send(message)
    }
    pub async fn summary(&self) -> Result<Session, Fault> {
        let response = self
            .codex
            .engine
            .call(
                "thread/read",
                json!({"threadId":self.binding.session.id,"includeTurns":false}),
            )
            .await?;
        binding::session(&response["thread"], &self.binding.session.cwd)
    }
    pub async fn shutdown(&self) {
        self.codex.shutdown().await;
    }
    pub fn stop(&self) {
        self.codex.stop();
    }
}
