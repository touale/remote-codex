use super::{Prepared, Thread};
use remote_codex_core::session::SessionSettings;
use remote_codex_protocol::Fault;
use serde_json::Value;
use std::{collections::BTreeMap, ffi::OsString, path::Path};

pub enum Action {
    Submit(String),
    Message {
        text: String,
        client_id: String,
        expected_turn: Option<String>,
    },
    Settings(SessionSettings),
    Interrupt(String),
}

impl Thread {
    pub fn prepare_action(
        &self,
        action: Action,
        persisted: bool,
        skills: &BTreeMap<String, String>,
    ) -> Result<Prepared, Fault> {
        let thread = &self.binding.session.id;
        let (method, params) = match action {
            Action::Submit(text) => ("turn/start", crate::events::prompt(thread, &text)),
            Action::Message {
                text,
                client_id,
                expected_turn,
            } => {
                if text.trim().is_empty()
                    || client_id.is_empty()
                    || client_id.len() > 128
                    || client_id.chars().any(char::is_control)
                {
                    return Err(Fault::new(
                        "INVALID_MESSAGE",
                        "A message requires text and a valid client ID.",
                    ));
                }
                let mut params = crate::events::prompt(thread, &text);
                params["clientUserMessageId"] = serde_json::json!(client_id);
                if let Some(turn) = expected_turn {
                    params["expectedTurnId"] = serde_json::json!(turn);
                    ("turn/steer", params)
                } else {
                    ("turn/start", params)
                }
            }
            Action::Settings(settings) => {
                if settings.mode.is_some() {
                    return Err(Fault::new(
                        "MODE_SETTINGS_REQUIRED",
                        "Mode changes require the confirmed model and effort.",
                    ));
                }
                (
                    "thread/settings/update",
                    crate::events::settings(thread, settings),
                )
            }
            Action::Interrupt(turn) => ("turn/interrupt", crate::events::interrupt(thread, &turn)),
        };
        self.prepare(method, params, persisted, skills)
    }

    pub fn frontend(
        &self,
        program: &crate::program::Launch,
        socket: &Path,
        arguments: &[OsString],
    ) -> Result<std::process::Command, Fault> {
        validate_frontend_arguments(arguments)?;
        let mut command = program.command();
        command
            .env("CODEX_HOME", &self.binding.codex_home)
            .current_dir(&self.binding.codex_home)
            .arg("--remote")
            .arg(format!("unix://{}", socket.display()))
            .arg("--cd")
            .arg(&self.binding.session.cwd)
            .arg("resume")
            .arg(&self.binding.session.id)
            .args(arguments);
        Ok(command)
    }
}

pub fn turn_id(response: &Value) -> Result<String, Fault> {
    response
        .pointer("/turn/id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            Fault::new(
                "INVALID_NATIVE_TURN",
                "native Codex did not return a turn ID",
            )
        })
}

fn validate_frontend_arguments(arguments: &[OsString]) -> Result<(), Fault> {
    for argument in arguments {
        let argument = argument.to_string_lossy();
        if ["--remote", "--cd", "--config"]
            .iter()
            .any(|flag| argument == *flag || argument.starts_with(&format!("{flag}=")))
            || argument.starts_with("-c")
            || argument.starts_with("-C")
        {
            return Err(Fault::new(
                "BOUND_FRONTEND_ARGUMENT",
                "remote transport, workspace and configuration overrides are managed by remote-codex",
            ));
        }
    }
    Ok(())
}
