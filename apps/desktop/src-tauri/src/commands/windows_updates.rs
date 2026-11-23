//! Windows installers exit the app, so installation follows the shared graceful restart flow.
use crate::{
    error::{Error, Result},
    state::AppState,
};
use remote_codex_client::application::{UpdateMode, UpdateProgress, UpdateSnapshot};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

pub(crate) struct Pending {
    update: Update,
    bytes: Vec<u8>,
}
fn failed(error: impl std::fmt::Display) -> Error {
    Error::new("UPDATE_FAILED", &error.to_string())
}

pub(crate) async fn snapshot(app: &AppHandle) -> Result<UpdateSnapshot> {
    let mut snapshot = super::updates::updater()?.snapshot().await?;
    snapshot.can_install = option_env!("REMOTE_CODEX_RELEASE") == Some("1");
    let state = app.state::<AppState>();
    let pending = state
        .pending_update
        .lock()
        .map_err(|_| crate::state::unavailable())?;
    snapshot.pending_version = pending.as_ref().map(|p| p.update.version.clone());
    snapshot.restart_required = pending.is_some();
    snapshot.available &= pending.as_ref().is_none_or(|p| {
        snapshot
            .latest
            .as_ref()
            .is_some_and(|l| l.version != p.update.version)
    });
    snapshot.busy |= state.update_gate.try_lock().is_err();
    Ok(snapshot)
}

pub(crate) async fn download(
    app: &AppHandle,
    report: impl Fn(UpdateProgress),
) -> Result<UpdateSnapshot> {
    let state = app.state::<AppState>();
    let gate = state
        .update_gate
        .try_lock()
        .map_err(|_| Error::new("UPDATE_BUSY", "An update is already running."))?;
    let status = snapshot(app).await?;
    if !status.can_install {
        return Err(Error::new(
            "UPDATE_UNMANAGED",
            "Install a GitHub Release to enable updates.",
        ));
    }
    let updater = app
        .updater_builder()
        .pubkey(include_str!("../../../../../crates/client/src/updates/public.key").trim())
        .endpoints(vec![
            url::Url::parse(
                "https://github.com/touale/remote-codex/releases/latest/download/latest.json",
            )
            .map_err(failed)?,
        ])
        .map_err(failed)?
        .build()
        .map_err(failed)?;
    if let Some(update) = updater.check().await.map_err(failed)? {
        let mut received = 0u64;
        let bytes = tokio::time::timeout(
            std::time::Duration::from_secs(600),
            update.download(
                |chunk, total| {
                    received = received.saturating_add(chunk as u64);
                    report(UpdateProgress {
                        phase: "downloading".into(),
                        received,
                        total: total.unwrap_or(0),
                    });
                },
                || {},
            ),
        )
        .await
        .map_err(failed)?
        .map_err(failed)?;
        *state
            .pending_update
            .lock()
            .map_err(|_| crate::state::unavailable())? = Some(Pending { update, bytes });
    }
    drop(gate);
    snapshot(app).await
}

pub(crate) async fn automatic(app: &AppHandle) -> Result<()> {
    let updater = super::updates::updater()?;
    let current = snapshot(app).await?;
    if current.mode == UpdateMode::Manual {
        return Ok(());
    }
    if current.last_checked.is_none_or(|checked| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(checked)
            >= remote_codex_client::application::UPDATE_CHECK_INTERVAL.as_secs()
    }) {
        updater.check().await?;
    }
    if current.mode == UpdateMode::Auto && current.can_install && !current.restart_required {
        download(app, |_| {}).await?;
    }
    Ok(())
}

pub(crate) fn install_pending(app: &AppHandle) -> Result<bool> {
    let state = app.state::<AppState>();
    let pending = state
        .pending_update
        .lock()
        .map_err(|_| crate::state::unavailable())?
        .take();
    if let Some(pending) = pending {
        pending.update.install(pending.bytes).map_err(failed)?;
        return Ok(true);
    }
    Ok(false)
}
