use dialoguer::{Confirm, FuzzySelect, Input, theme::ColorfulTheme};
use remote_codex_client::{ClientError, Result};
use std::io::IsTerminal;

pub(crate) fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

pub(crate) fn text(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(160)
        .collect()
}

pub(crate) fn input(prompt: &str) -> Result<String> {
    if !interactive() {
        return Err(ClientError::Argument(
            "required option missing; this command cannot prompt without a terminal",
        ));
    }
    Input::<String>::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .interact_text()
        .map_err(|_| ClientError::Argument("input cancelled"))
}

pub(crate) fn validated_input(
    prompt: &str,
    validator: impl FnMut(&String) -> std::result::Result<(), String>,
) -> Result<String> {
    if !interactive() {
        return Err(ClientError::Argument("input requires a terminal"));
    }
    Input::<String>::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .report(false)
        .validate_with(validator)
        .interact_text()
        .map_err(|_| ClientError::Argument("input cancelled"))
}

pub(crate) fn confirm(prompt: &str, default: bool) -> Result<bool> {
    if !interactive() {
        return Err(ClientError::Argument(
            "confirmation requires a terminal or an explicit command option",
        ));
    }
    Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(default)
        .interact()
        .map_err(|_| ClientError::Argument("confirmation cancelled"))
}

pub(crate) fn choose(prompt: &str, items: &[String]) -> Result<usize> {
    if !interactive() {
        return Err(ClientError::Argument(
            "selection requires an explicit server, path or session ID without a terminal",
        ));
    }
    if items.is_empty() {
        return Err(ClientError::NotFound);
    }
    let items: Vec<_> = items.iter().map(|s| text(s)).collect();
    FuzzySelect::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(&items)
        .default(0)
        .interact_opt()
        .map_err(|_| ClientError::Argument("selection cancelled"))?
        .ok_or(ClientError::Argument("selection cancelled"))
}
