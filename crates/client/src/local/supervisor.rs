use super::*;
use remote_codex_core::goals::Goal;
use remote_codex_protocol::{Fault, Request};

impl LocalRuntime {
    pub(super) fn cancel_continuation(&self, turn: &str) -> Result<()> {
        self.intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .cancel(turn);
        Ok(())
    }

    pub(crate) fn retry(&self) {
        self.recovery.retry();
    }

    pub(crate) async fn authenticate(&self) -> Result<()> {
        if !matches!(&*self.recovery.state.borrow(), EnvironmentState::ActionRequired { code, .. } if code == "SSH_AUTH_REQUIRED")
        {
            return Err(ClientError::Argument(
                "this session is not waiting for SSH authentication",
            ));
        }
        self.remote.recover(true).await?;
        self.retry();
        Ok(())
    }

    pub(super) async fn rebuild(
        &self,
        old: &Generation,
        reason: String,
    ) -> Result<Arc<Generation>> {
        if self.closed.borrow().is_some() || *self.lease.revoked.borrow() {
            return Err(
                Fault::new("SESSION_CLOSED", "session control is no longer available").into(),
            );
        }
        self.recovery.begin_rebuild();
        self.intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .disconnect();
        self.recovery.publish(EnvironmentState::Recovering {
            reason: reason.clone(),
        });
        self.announce(&format!(
            "Execution environment interrupted ({reason}). Restoring this session…"
        ));
        // Resolve stale UI prompts before shutting down their owning native process.
        if let Ok(mut pending) = old.pending_requests.lock() {
            for id in pending.keys() {
                if let Ok(event) =
                    remote_codex_adapter::events::resolved(&self.binding.session.id, id)
                {
                    if let Some(public) = remote_codex_adapter::events::public_event(&event) {
                        let _ = self.events.send(public);
                    }
                    let _ = self.native_events.send(event);
                }
            }
            pending.clear();
        }
        if let Err(error) = self.suspend_goal().await {
            self.announce(&format!(
                "Suspending interrupted goal during recovery: {error}"
            ));
        }
        let full_access = old.permissions.full_access();
        old.close().await;
        let active = self
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .active
            .clone();
        if let Some(turn) = active {
            let event = remote_codex_adapter::events::interrupted(&self.binding.session.id, &turn);
            if let Some(public) = remote_codex_adapter::events::public_event(&event) {
                let _ = self.events.send(public);
            }
            let _ = self.native_events.send(event);
        }
        let mut error = ClientError::Ssh(255);
        loop {
            self.recovery.wait(&error).await?;
            match self.replace(old, full_access).await {
                Ok((generation, goal)) => {
                    if self.closed.borrow().is_some() || *self.lease.revoked.borrow() {
                        generation.close().await;
                        return Err(Fault::new(
                            "SESSION_CLOSED",
                            "session control is no longer available",
                        )
                        .into());
                    }
                    let generation = Arc::new(generation);
                    {
                        let mut intent = self
                            .intent
                            .lock()
                            .map_err(|_| ClientError::RemoteResponse)?;
                        intent.goal = goal.clone();
                        intent.goal_revision = intent.goal_revision.wrapping_add(1);
                    }
                    let _ = self.events.send(SessionEvent::GoalChanged { goal });
                    *self
                        .generation
                        .write()
                        .map_err(|_| ClientError::RemoteResponse)? = generation.clone();
                    self.recovery.publish(EnvironmentState::Recovering {
                        reason: "execution environment restored".into(),
                    });

                    self.announce("Execution environment restored. Checking interrupted work…");
                    return Ok(generation);
                }
                Err(fault) => error = fault,
            }
        }
    }

    async fn replace(
        &self,
        old: &Generation,
        full_access: bool,
    ) -> Result<(Generation, Option<Goal>)> {
        self.remote.recover(false).await?;
        let project = self
            .remote
            .call(Request::ProjectConfig {
                path: self.recipe.cwd.clone(),
            })
            .await?;
        if project != self.recipe.project {
            return Err(Fault::new("PROJECT_CHANGED", "project configuration changed; review it and resume this session before continuing").into());
        }
        let snapshot = self.store.config_snapshot(&self.remote.server.id).await?;
        for (key, _) in snapshot
            .effective
            .entries()
            .chain(self.recipe.config.entries())
        {
            if matches!(
                key,
                crate::config::ConfigKey::Background
                    | crate::config::ConfigKey::DisconnectGraceSeconds
                    | crate::config::ConfigKey::ReconnectMaxAttempts
            ) {
                continue;
            }
            if snapshot.effective.get(key) != self.recipe.config.get(key) {
                return Err(Fault::new(
                    "CONFIGURATION_CHANGED",
                    "execution configuration changed; reopen this session to apply it",
                )
                .into());
            }
        }
        self.remote.synchronize(&self.store).await?;
        let mut recipe = self.recipe.clone();
        recipe.revision = snapshot.revision.saved;
        let (mut generation, _) = recipe
            .open(
                self.remote.clone(),
                Some(&self.binding),
                self.recovery.clone(),
                &|_| {},
                Some(&old.skills),
            )
            .await?;
        let verified = async {
            let settings = self
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?
                .settings
                .clone();
            let full = generation
                .native
                .restore_settings(&settings, full_access)
                .await?;
            generation.permissions.restore(
                &generation.bridge.channel,
                &self.binding.session.id,
                full,
            )?;
            if self.has_history.load(Ordering::Acquire) {
                self.refresh_summary(&generation).await?;
            }
            Ok::<_, ClientError>(generation.native.goal().await?)
        }
        .await;
        match verified {
            Ok(goal) => Ok((generation, goal)),
            Err(error) => {
                generation.close().await;
                Err(error)
            }
        }
    }

    pub(super) async fn continue_interrupted(&self, rebuilt: bool) -> Result<()> {
        let continuation = self
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .continuation(rebuilt);
        let Some((episode, turn)) = continuation else {
            return Ok(());
        };
        if self.closed.borrow().is_some() || *self.lease.revoked.borrow() {
            return Ok(());
        }
        // This marker is durable in native history. Never retry turn/start on an
        // ambiguous acknowledgement: a retry could schedule the task twice.
        let prompt = format!(
            "[remote-codex recovery {episode}] Automatic continuation after the execution environment reconnected. Continue the user's unfinished task. First inspect remote_jobs and existing workspace results without changing them: previous commands may have completed or may still be running. Do not repeat commands, patches or MCP calls whose outcome cannot be verified; ask the user instead. Preserve the current permissions and the user's latest instructions."
        );
        let submitted = self.submit_recovery(prompt, &turn).await;
        if let Ok(id) = &submitted {
            let mut intent = self
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            if intent.completed.as_ref() != Some(id) {
                intent.active = Some(id.clone());
            }
        }
        if let Err(error) = submitted {
            if error.code() == "RECOVERY_CANCELLED" {
                return Ok(());
            }
            if self
                .current()?
                .native
                .recovery_recorded(&episode)
                .await
                .unwrap_or(false)
            {
                return Ok(());
            }
            self.recovery.publish(EnvironmentState::ActionRequired { code: "CONTINUATION_UNCONFIRMED".into(), message: format!("Recovery was submitted once but could not be confirmed: {error}. Inspect this session before continuing.") });
        }
        Ok(())
    }

    pub(super) fn announce(&self, message: &str) {
        let _ = self
            .native_events
            .send(remote_codex_adapter::events::warning(
                &self.binding.session.id,
                message,
            ));
    }
}
