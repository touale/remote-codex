use crate::{
    args::{Cli, UpdateCommand},
    output,
};
use remote_codex_client::{
    ClientError, Result,
    application::{UPDATE_CHECK_INTERVAL, UpdateComponent, UpdateMode, UpdateSnapshot, Updater},
};
use std::{
    io::{IsTerminal, Write},
    path::PathBuf,
};

pub(crate) async fn run(cli: &Cli, command: Option<&UpdateCommand>) -> Result<()> {
    let updater = Updater::open(cli.data_dir.clone(), UpdateComponent::Cli)?;
    let snapshot = match command {
        Some(UpdateCommand::Check) => updater.check().await?,
        Some(UpdateCommand::Status) => updater.snapshot().await?,
        Some(UpdateCommand::Config { mode }) => match mode.as_deref() {
            Some(value) => {
                updater
                    .configure(match value {
                        "auto" => UpdateMode::Auto,
                        "manual" => UpdateMode::Manual,
                        _ => UpdateMode::Notify,
                    })
                    .await?
            }
            None => updater.snapshot().await?,
        },
        None => {
            if !cli.json {
                display(&updater.snapshot().await?, false)?;
            }
            let report = |p: remote_codex_client::application::UpdateProgress| {
                if cli.json {
                    return;
                }
                let text = format!(
                    "{} {}% ({:.1}/{:.1} MiB)",
                    p.phase,
                    p.received
                        .saturating_mul(100)
                        .checked_div(p.total)
                        .unwrap_or(0),
                    p.received as f64 / 1048576.0,
                    p.total as f64 / 1048576.0
                );
                if std::io::stderr().is_terminal() {
                    eprint!("\rremote-codex: {text}\x1b[K");
                    let _ = std::io::stderr().flush();
                } else {
                    eprintln!("remote-codex: {text}");
                }
            };
            let result = tokio::select! {
                result = updater.install(&report) => result,
                _ = tokio::signal::ctrl_c() => Err(ClientError::Argument("update cancelled")),
            };
            if !cli.json && std::io::stderr().is_terminal() {
                eprintln!();
            }

            result?
        }
    };
    display(&snapshot, cli.json)
}

fn display(snapshot: &UpdateSnapshot, json: bool) -> Result<()> {
    if json {
        return output::json(snapshot);
    }
    println!(
        "Update mode: {}",
        match snapshot.mode {
            UpdateMode::Notify => "notify",
            UpdateMode::Auto => "auto",
            UpdateMode::Manual => "manual",
        }
    );
    println!(
        "Latest release: {}",
        snapshot
            .latest
            .as_ref()
            .map(|r| r.version.as_str())
            .unwrap_or("Not checked")
    );
    println!(
        "CLI  {}  {}",
        snapshot.installed_version,
        snapshot.path.display()
    );
    if snapshot.restart_required {
        println!("Update installed. The next launch uses the new version.");
    }
    if !snapshot.can_install {
        println!("Development build: install a GitHub Release to enable updates.");
    }
    Ok(())
}

pub(crate) fn background(directory: Option<PathBuf>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Ok(updater) = Updater::open(directory, UpdateComponent::Cli) {
            loop {
                let _ = updater.automatic_check().await;
                tokio::time::sleep(UPDATE_CHECK_INTERVAL).await;
            }
        }
    })
}

pub(crate) async fn notice(directory: Option<PathBuf>) {
    if let Ok(updater) = Updater::open(directory, UpdateComponent::Cli)
        && let Ok(s) = updater.snapshot().await
    {
        if s.restart_required {
            eprintln!(
                "remote-codex: Update installed; next launch uses {}.",
                s.installed_version
            );
        } else if s.mode != UpdateMode::Manual
            && s.available
            && let Some(release) = s.latest
        {
            eprintln!(
                "remote-codex: Release {} is available. Run `remote-codex update` to install.",
                release.version
            );
        }
    }
}
