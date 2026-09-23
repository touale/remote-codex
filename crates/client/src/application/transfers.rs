use super::Client;
pub use crate::transfers::{Choice, Conflict, Direction, Progress, Status, Transfer};
use crate::{
    ClientError, Result,
    transfers::{self, lease::Lease},
};
use std::sync::Arc;

#[derive(serde::Serialize)]
pub struct SkippedTransfers {
    pub paths: Vec<String>,
    pub next: Option<i64>,
}

pub struct TransferService {
    pub(super) client: Client,
}
impl TransferService {
    pub async fn remove_finished(&self, ids: &[String]) -> Result<Vec<String>> {
        let state = &self.client.0;
        state.ensure_open()?;
        let mut removable = Vec::new();
        let mut leases = Vec::new();
        for id in ids.iter().collect::<std::collections::BTreeSet<_>>() {
            if let Some(lease) = Lease::try_acquire(&state.directory, id)? {
                leases.push(lease);
                removable.push(id.clone());
            }
        }
        state.store.transfer_remove_finished(&removable).await
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.client.0.transfers.owns(id)
    }
    /// IDs of transfers currently owned by this client.
    pub fn running_ids(&self) -> Result<Vec<String>> {
        self.client.0.transfers.running_ids()
    }
    pub async fn list(&self) -> Result<Vec<Transfer>> {
        let state = &self.client.0;
        let mut tasks = state.store.transfers().await?;
        for task in &mut tasks {
            (task.view.bytes, task.view.completed, task.view.skipped) =
                state.store.transfer_progress(&task.view.id).await?;
            task.view.owned = state.transfers.owns(&task.view.id);
            task.view.active =
                task.view.owned || Lease::acquire(&state.directory, &task.view.id).is_err();
            if !task.view.active
                && matches!(
                    task.view.status,
                    Status::Queued | Status::Running | Status::Preparing | Status::Reconnecting
                )
            {
                task.view.status = Status::Paused;
            }
        }
        Ok(tasks.into_iter().map(|t| t.view).collect())
    }
    pub async fn run(&self, id: &str, progress: Progress) -> Result<()> {
        let state = &self.client.0;
        state.ensure_open()?;
        let registration = state.transfers.register(id)?;
        // The lease drops before registration notifies waiting frontends.
        let lease = Arc::new(Lease::acquire(&state.directory, id)?);
        let mut queued = state.store.transfer(id).await?;
        if matches!(queued.view.status, Status::Completed | Status::Cancelled) {
            return Ok(());
        }
        let _workspace = crate::workspace_lock::WorkspaceLock::acquire(
            &state.directory,
            &queued.server_id,
            Some(&queued.view.workspace),
            false,
        )?;
        queued.view.status = Status::Queued;
        queued.view.active = true;
        queued.view.owned = true;
        transfers::engine::publish(&state.store, &queued, &progress).await?;
        let execute = async {
            let _slot = transfers::runtime::slot().await?;
            self.execute(id, lease.clone(), &progress).await
        };
        let result = tokio::select! {
            result = execute => result,
            _ = registration.control.cancelled() => Ok(()),
        };
        // Cancelled blocking filesystem work retains the lease until its next block boundary.
        while Arc::strong_count(&lease) > 1 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let mut task = state.store.transfer(id).await?;
        (task.view.bytes, task.view.completed, task.view.skipped) =
            state.store.transfer_progress(id).await?;
        task.view.active = false;
        task.view.owned = false;
        if !matches!(
            task.view.status,
            Status::Conflict | Status::Completed | Status::Cancelled
        ) {
            task.view.status = Status::Paused;
        }
        if let Err(error) = &result {
            task.view.message = Some(error.to_string());
            task.view.error_code = Some(error.code().into());
        }
        transfers::engine::publish(&state.store, &task, &progress).await?;
        result
    }
    async fn execute(&self, id: &str, lease: Arc<Lease>, progress: &Progress) -> Result<()> {
        let state = &self.client.0;
        state.ensure_open()?;
        let mut task = state.store.transfer(id).await?;
        if matches!(task.view.status, Status::Completed | Status::Cancelled) {
            return Ok(());
        }
        task.view.active = true;
        task.view.owned = true;
        task.view.status = Status::Preparing;
        task.view.message = task
            .cancel
            .then(|| "Removing temporary transfer data…".into());
        task.view.error_code = None;
        transfers::engine::publish(&state.store, &task, progress).await?;
        let record = state.store.connection_by_id(&task.server_id).await?;
        let remote = state.connect(record).await?;
        let mut attempt = 0u32;
        loop {
            let result = match remote.recover_silent().await {
                Ok(_) => {
                    transfers::engine::run(&state.store, id, &remote, lease.clone(), progress).await
                }
                Err(error) => Err(error),
            };
            match result {
                Ok(()) => return Ok(()),
                Err(error) if crate::remote::recovery::transient(&error) => {
                    task = state.store.transfer(id).await?;
                    task.view.status = Status::Reconnecting;
                    task.view.message =
                        Some("Connection interrupted. Waiting to reconnect…".into());
                    transfers::engine::publish(&state.store, &task, progress).await?;
                    tokio::time::sleep(std::time::Duration::from_secs(
                        (1u64 << attempt.min(5)).min(30),
                    ))
                    .await;
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }
    pub async fn pause(&self, id: &str) -> Result<()> {
        self.client.0.transfers.pause(id).await;
        let _lease = Lease::acquire(&self.client.0.directory, id)?;
        Ok(())
    }
    pub async fn resolve(&self, id: &str, choice: Choice, all: bool) -> Result<()> {
        let state = &self.client.0;
        let _lease = Lease::acquire(&state.directory, id)?;
        let mut task = state.store.transfer(id).await?;
        let conflict = task.view.conflict.as_ref().ok_or(ClientError::Argument(
            "This transfer has no pending conflict.",
        ))?;
        let valid = match choice {
            Choice::Replace => {
                conflict.source == remote_codex_protocol::transfer::Kind::File
                    && conflict.destination == conflict.source
            }
            Choice::Merge => {
                conflict.source == remote_codex_protocol::transfer::Kind::Directory
                    && conflict.destination == conflict.source
            }
            _ => true,
        };
        if !valid {
            return Err(ClientError::Argument(
                "This choice is not available for these file types.",
            ));
        }
        let mut item = state
            .store
            .transfer_items(id)
            .await?
            .into_iter()
            .find(|i| Some(i.index) == task.current)
            .ok_or(ClientError::RemoteResponse)?;
        item.choice = Some(choice);
        item.restage = item.prepared;
        state.store.transfer_item(id, &item).await?;
        if all {
            task.policy = Some(choice);
        }
        state.store.transfer_save(&task).await
    }
    pub async fn restart_file(&self, id: &str) -> Result<()> {
        let state = &self.client.0;
        let _lease = Lease::acquire(&state.directory, id)?;
        let mut task = state.store.transfer(id).await?;
        let mut item = state
            .store
            .transfer_items(id)
            .await?
            .into_iter()
            .find(|i| Some(i.index) == task.current && !i.done)
            .ok_or(ClientError::Argument("No incomplete file to restart."))?;
        item.restart = true;
        task.view.conflict = None;
        state.store.transfer_item(id, &item).await?;
        state.store.transfer_save(&task).await
    }
    pub async fn cancel(&self, id: &str) -> Result<()> {
        self.pause(id).await?;
        let state = &self.client.0;
        let _lease = Lease::acquire(&state.directory, id)?;
        let mut task = state.store.transfer(id).await?;
        task.cancel = true;
        task.view.status = Status::Paused;
        task.view.message =
            Some("Cancellation requested. Removing temporary transfer data…".into());
        task.view.conflict = None;
        state.store.transfer_save(&task).await
    }
    pub async fn skipped(&self, id: &str, after: Option<i64>) -> Result<SkippedTransfers> {
        let mut entries = self
            .client
            .0
            .store
            .transfer_skipped(id, after.unwrap_or(-1))
            .await?;
        let more = entries.len() > 100;
        if more {
            entries.pop();
        }
        let next = if more {
            entries.last().map(|(index, _)| *index)
        } else {
            None
        };
        Ok(SkippedTransfers {
            paths: entries.into_iter().map(|(_, path)| path).collect(),
            next,
        })
    }
    pub async fn download_directory(&self, id: &str) -> Result<std::path::PathBuf> {
        let task = self.client.0.store.transfer(id).await?;
        if task.view.direction != Direction::Download {
            return Err(ClientError::Argument("This is not a download."));
        }
        Ok(task
            .grants
            .first()
            .ok_or(ClientError::RemoteResponse)?
            .root
            .clone())
    }
}
