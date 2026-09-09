mod servers;

pub(crate) use servers::server_list;

use remote_codex_client::{
    ClientError, Result,
    config::{ConfigReport, VisibleValue},
};
use serde::Serialize;

pub(crate) fn notice(
    writer: &mut impl std::io::Write,
    notice: remote_codex_client::application::ClientNotice,
    json: bool,
) -> std::io::Result<()> {
    if json {
        let mut value = serde_json::to_value(notice)?;
        value["level"] = serde_json::json!("warning");
        value["message"] = serde_json::json!(notice.message());
        writeln!(writer, "{value}")
    } else {
        writeln!(writer, "remote-codex: warning: {}", notice.message())
    }
}

pub(crate) fn json(value: &impl Serialize) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({"schema_version":4,"data":value}))?
    );
    Ok(())
}

pub(crate) fn error(error: &ClientError) -> Result<()> {
    failure(
        error.code(),
        &error.to_string(),
        error.retryable(),
        error.outcome_is_unknown(),
    )
}

pub(crate) fn failure(
    code: &str,
    message: &str,
    retryable: bool,
    outcome_unknown: bool,
) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({"schema_version":4,"error":{
            "code":code,"message":message,"retryable":retryable,"outcome_unknown":outcome_unknown
        }}))?
    );
    Ok(())
}

pub(crate) fn visible(value: Option<&VisibleValue>) -> Result<String> {
    match value {
        None => Ok("<unset>".to_owned()),
        Some(VisibleValue::Text(text)) if !text.chars().any(char::is_control) => Ok(text.clone()),
        Some(value) => Ok(serde_json::to_string(value)?),
    }
}

pub(crate) fn config(report: &ConfigReport, as_json: bool, get: bool) -> Result<()> {
    if as_json {
        return json(report);
    }
    for item in &report.items {
        let value = if item.setting.redacted {
            "<redacted>".to_owned()
        } else {
            visible(item.setting.value.as_ref())?
        };
        if get {
            println!("{value}");
        } else {
            println!(
                "{} = {}  ({:?}; {}){}",
                item.setting.key,
                value,
                item.setting.source,
                item.application_state,
                if item.setting.masked_by_proxy_mode {
                    " [masked by direct mode]"
                } else {
                    ""
                }
            );
        }
    }
    if !get {
        println!(
            "saved revision: {}; applied revision: {}",
            report.saved_revision,
            report
                .applied_revision
                .map(|n| n.to_string())
                .unwrap_or_else(|| "not acknowledged".into())
        );
    }
    Ok(())
}
