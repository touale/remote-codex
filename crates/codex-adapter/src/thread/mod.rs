pub mod action;
mod attachments;
pub(crate) mod binding;
mod creation;
mod execution_policy;
mod goal_attachments;
pub mod history;
mod request;
mod settings;

use crate::engine::Engine;
pub use creation::Creation;
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
    home: PathBuf,
}

#[derive(Clone, Copy)]
pub enum OpenSource<'a> {
    New,
    Resume(&'a str),
    Frontend(&'a Creation),
}

pub struct OpenThread<'a> {
    pub environment: &'a str,
    pub directory: &'a str,
    pub execution_mode: &'a str,
    pub source: OpenSource<'a>,
    pub mcp: BTreeMap<String, Value>,
    pub instructions: String,
}

pub struct Opened {
    pub session: Session,
    pub full_access: bool,
    response: Value,
    usage: Option<remote_codex_core::status::TokenUsage>,
    events: tokio::sync::broadcast::Receiver<Value>,
}

pub struct Thread {
    pub(crate) codex: Codex,
    pub(crate) binding: SessionBinding,
    bootstrap: Value,
    initial_usage: Option<remote_codex_core::status::TokenUsage>,
    initial_events: std::sync::Mutex<Option<tokio::sync::broadcast::Receiver<Value>>>,
}

impl Codex {
    pub async fn start(program: &Path, home: &Path) -> Result<Self, Fault> {
        let (engine, initialized) = Engine::local(program, home).await?;
        Ok(Self {
            engine,
            initialized,
            home: home.into(),
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
        {
            let native = self
                .engine
                .call("config/read", json!({"includeLayers":false}))
                .await?;
            let inherited = native["config"]["developer_instructions"]
                .as_str()
                .unwrap_or_default();
            params["developerInstructions"] = json!(format!(
                "{inherited}\n\n{}\n\n{}",
                options.instructions,
                goal_attachments::INSTRUCTIONS
            ));
        }
        params["dynamicTools"] = json!([crate::recovery::tool(), goal_attachments::tool()]);
        let events = self.engine.subscribe();
        let response = if let OpenSource::Resume(id) = options.source {
            if let Some(object) = params.as_object_mut() {
                for key in ["environments", "sandbox", "approvalPolicy"] {
                    object.remove(key);
                }
            }
            params["threadId"] = json!(id);
            params["excludeTurns"] = json!(true);
            self.engine.call("thread/resume", params).await?
        } else if let OpenSource::Frontend(creation) = options.source {
            let method = creation.apply(&mut params);
            self.engine.call(method, params).await?
        } else {
            self.engine.call("thread/start", params).await?
        };
        let session = binding::session(&response["thread"], options.directory)?;
        let usage = if matches!(options.source, OpenSource::Resume(_))
            || matches!(options.source, OpenSource::Frontend(creation) if creation.is_fork())
        {
            crate::usage_history::read(&self.home, &session.id, response["thread"]["path"].as_str())
                .await
        } else {
            None
        };
        Ok(Opened {
            session,
            usage,
            full_access: response.pointer("/sandbox/type").and_then(Value::as_str)
                == Some("dangerFullAccess"),
            response,
            events,
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
            initial_usage: opened.usage,
            initial_events: std::sync::Mutex::new(Some(opened.events)),
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
    pub fn bootstrap(&self) -> Value {
        self.bootstrap.clone()
    }
    pub fn binding(&self) -> &SessionBinding {
        &self.binding
    }

    /// Subscribe before opening the thread so startup/resume notifications are not lost.
    /// One owner drains this bounded native broadcast stream; other subscribers see live events.
    pub fn take_events(&self) -> Result<tokio::sync::broadcast::Receiver<Value>, Fault> {
        self.initial_events
            .lock()
            .map_err(|_| Fault::new("SESSION_STATE", "Native event stream unavailable"))?
            .take()
            .ok_or_else(|| Fault::new("EVENT_OWNER", "Native event stream already claimed"))
    }
    pub fn initial_status(&self) -> remote_codex_core::status::SessionStatus {
        remote_codex_core::status::SessionStatus {
            usage: self.initial_usage.clone(),
            ..crate::status::initial(&self.bootstrap)
        }
    }
    pub fn settings_snapshot(&self) -> Value {
        let mut settings = self.bootstrap.clone();
        settings["sandboxPolicy"] = settings["sandbox"].clone();
        settings["effort"] = settings["reasoningEffort"].clone();
        settings
    }

    pub async fn restore_settings(
        &mut self,
        snapshot: &Value,
        full_access: bool,
    ) -> Result<bool, Fault> {
        let params = settings::restore(snapshot, &self.binding, full_access)?;
        let mut events = self.subscribe();
        self.codex
            .engine
            .call("thread/settings/update", params)
            .await?;
        let confirmed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let event = events
                    .recv()
                    .await
                    .map_err(|_| Fault::unknown("native settings confirmation lost"))?;
                if event["method"] == "thread/settings/updated"
                    && event["params"]["threadId"] == self.binding.session.id
                {
                    return Ok::<_, Fault>(event["params"]["threadSettings"].clone());
                }
            }
        })
        .await
        .map_err(|_| Fault::unknown("native settings were not confirmed after recovery"))??;
        if let Some(settings) = confirmed.as_object() {
            for (key, value) in settings {
                self.bootstrap[key] = value.clone();
            }
        }
        self.bootstrap["sandbox"] = confirmed["sandboxPolicy"].clone();
        self.bootstrap["reasoningEffort"] = confirmed["effort"].clone();
        Ok(confirmed["sandboxPolicy"]["type"] == "dangerFullAccess")
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Value> {
        self.codex.engine.subscribe()
    }
    pub async fn recovery_recorded(&self, episode: &str) -> Result<bool, Fault> {
        let turns = self
            .codex
            .engine
            .call(
                "thread/turns/list",
                json!({"threadId":self.binding.session.id,"limit":8}),
            )
            .await?;
        let marker = format!("[remote-codex recovery {episode}]");
        Ok(turns["data"].as_array().is_some_and(|turns| {
            turns.iter().any(|turn| {
                turn["items"].as_array().is_some_and(|items| {
                    items.iter().any(|item| {
                        item["type"] == "userMessage" && item.to_string().contains(&marker)
                    })
                })
            })
        }))
    }
    pub fn send(&self, message: Value) -> Result<(), Fault> {
        self.codex.engine.send(message)
    }
    pub fn queue_interrupt(&self, turn: &str) -> Result<(), Fault> {
        drop(self.codex.engine.begin(
            "turn/interrupt",
            crate::events::interrupt(&self.binding.session.id, turn),
        )?);
        Ok(())
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
