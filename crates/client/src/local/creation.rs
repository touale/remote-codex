use super::*;
use remote_codex_adapter::thread::{Creation, OpenSource};
use remote_codex_protocol::Fault;

impl LocalRuntime {
    pub(crate) async fn create_from_frontend(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<(Arc<Self>, Value)> {
        let _goal = self.goal_gate.lock().await;
        let _turn = self.turn_gate.lock().await;
        if self.closed.borrow().is_some() || *self.lease.revoked.borrow() {
            return Err(
                Fault::new("SESSION_CLOSED", "session control is no longer available").into(),
            );
        }
        self.recovery.ready()?;
        let source = self.current()?;
        source.bridge.check()?;
        let project = self
            .remote
            .call(remote_codex_protocol::Request::ProjectConfig {
                path: self.recipe.cwd.clone(),
            })
            .await?;
        if project != self.recipe.project {
            return Err(Fault::new("PROJECT_CHANGED", "project configuration changed; review it and resume this session before editing a prompt").into());
        }
        let busy = {
            let intent = self
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            intent.active.is_some()
                || intent
                    .goal
                    .as_ref()
                    .is_some_and(|goal| goal.status == remote_codex_core::goals::GoalStatus::Active)
        };
        if busy {
            return Err(Fault::new(
                "TURN_ACTIVE",
                "Stop the current turn and pause its goal before editing a prompt.",
            )
            .into());
        }
        let creation = Creation::prepare(
            method,
            params,
            source.native.binding(),
            source.permissions.full_access(),
        )?;
        let workspace_lock = crate::workspace_lock::WorkspaceLock::acquire(
            &self.store.directory,
            &self.binding.server_id,
            Some(&self.recipe.cwd),
            false,
        )?;
        let recovery = Arc::new(Recovery::new(
            self.store.clone(),
            self.binding.server_id.clone(),
        ));
        let generation = self
            .recipe
            .open(
                self.remote.clone(),
                OpenSource::Frontend(&creation),
                recovery.clone(),
                &|_| {},
                Some(&source.skills),
            )
            .await?;
        let response = generation.native.bootstrap();
        let id = generation.native.binding().session.id.clone();
        let ready = Self::from_generation(
            &self.store, self.remote.clone(), self.recipe.clone(), generation, recovery,
            (None, workspace_lock), creation.is_fork(),
        ).await.map_err(|error| Fault::unknown(&format!(
            "Codex created thread {id}, but its remote binding could not be prepared: {error}. The source session was kept; do not repeat creation automatically."
        )))?;
        if self.closed.borrow().is_some()
            || *self.lease.revoked.borrow()
            || self.current()?.bridge.channel != source.bridge.channel
        {
            ready.shutdown().await;
            return Err(Fault::unknown(&format!("Thread {id} was created, but the source connection changed before attachment. Resume the saved thread explicitly.")).into());
        }
        Ok((ready, response))
    }
}
