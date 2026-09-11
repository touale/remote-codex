use super::{LocalRuntime, route};
use crate::{ClientError, Result};
use remote_codex_adapter::{events, goals};
use remote_codex_core::{
    goals::{CollaborationMode, Goal, GoalAction, GoalStatus},
    session::{SessionEvent, SessionSettings},
};
use remote_codex_protocol::Fault;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

impl LocalRuntime {
    pub(super) async fn load_goal(&self) -> Result<Option<Goal>> {
        let revision = self
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .goal_revision;
        let goal = self.current()?.native.goal().await?;
        let mut intent = self
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?;
        // A notification received during the read is newer than this request's starting snapshot.
        if intent.goal_revision == revision {
            intent.goal = goal;
            intent.goal_revision = intent.goal_revision.wrapping_add(1);
        }
        Ok(intent.goal.clone())
    }
    fn idle(&self) -> Result<()> {
        if self
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .active
            .is_some()
        {
            return Err(Fault::new(
                "TURN_ACTIVE",
                "Stop the current turn before changing mode or starting a goal.",
            )
            .into());
        }
        Ok(())
    }
    async fn goal_request(&self, method: &str, params: Value) -> Result<Option<Goal>> {
        let generation = self.current()?;
        let prepared = generation.native.prepare(
            method,
            params,
            generation.permissions.full_access(),
            self.has_history.load(Ordering::Acquire),
            generation.skills.mappings(),
        )?;
        route::execute(self, &generation, prepared).await?;
        let goal = self.load_goal().await?;
        let _ = self
            .events
            .send(SessionEvent::GoalChanged { goal: goal.clone() });
        Ok(goal)
    }
    pub(crate) async fn goal(&self, action: GoalAction) -> Result<Option<Goal>> {
        let _gate = self.goal_gate.lock().await;
        if matches!(action, GoalAction::Set { .. } | GoalAction::Resume) {
            self.idle()?;
        }
        if !matches!(action, GoalAction::Budget { .. }) {
            self.intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?
                .resume_goal = None;
        }
        if matches!(action, GoalAction::Resume)
            && self.current_settings()?.mode == CollaborationMode::Plan
        {
            self.change_mode_locked(SessionSettings {
                mode: Some(CollaborationMode::Agent),
                ..Default::default()
            })
            .await?;
        }
        let set = matches!(action, GoalAction::Set { .. });
        let (method, mut params) = goals::params(&self.binding.session.id, action)?;
        if set
            && (self.current_settings()?.mode == CollaborationMode::Plan
                || self
                    .intent
                    .lock()
                    .map_err(|_| ClientError::RemoteResponse)?
                    .goal
                    .as_ref()
                    .is_some_and(|g| {
                        !matches!(g.status, GoalStatus::Active | GoalStatus::Complete)
                    }))
        {
            params["status"] = json!("paused");
        }
        self.goal_request(method, params).await
    }
    async fn pause_goal_locked(&self) -> Result<()> {
        let active = self
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .goal
            .as_ref()
            .is_some_and(|g| g.status == GoalStatus::Active);
        if active {
            let (method, params) = goals::params(&self.binding.session.id, GoalAction::Pause)?;
            self.goal_request(method, params).await?;
        }
        Ok(())
    }
    pub(super) async fn pause_goal(&self) -> Result<()> {
        let _gate = self.goal_gate.lock().await;
        self.intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .resume_goal = None;
        self.pause_goal_locked().await
    }
    pub(super) async fn suspend_goal(&self) -> Result<()> {
        let _gate = self.goal_gate.lock().await;
        let objective = self
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .goal
            .as_ref()
            .filter(|g| g.status == GoalStatus::Active)
            .map(|g| g.objective.clone());
        if let Some(objective) = objective {
            self.intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?
                .resume_goal = Some(objective);
            self.pause_goal_locked().await?;
        }
        Ok(())
    }
    pub(super) async fn restore_goal(&self) -> Result<()> {
        let _gate = self.goal_gate.lock().await;
        if self.recovery.ready().is_err() {
            return Ok(());
        }
        let resume = {
            let mut intent = self
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            if intent.active.is_some() || intent.interrupted.is_some() || intent.episode.is_some() {
                return Ok(());
            }
            let objective = intent.resume_goal.take();
            objective.is_some()
                && intent.goal.as_ref().is_some_and(|g| {
                    Some(&g.objective) == objective.as_ref() && g.status == GoalStatus::Paused
                })
        };
        if resume {
            let (method, params) = goals::params(&self.binding.session.id, GoalAction::Resume)?;
            self.goal_request(method, params).await?;
        }
        Ok(())
    }
    pub(super) async fn change_mode(&self, settings: SessionSettings) -> Result<()> {
        let _gate = self.goal_gate.lock().await;
        self.change_mode_locked(settings).await
    }
    async fn change_mode_locked(&self, mut settings: SessionSettings) -> Result<()> {
        self.idle()?;
        let mode = settings.mode.take().ok_or(ClientError::RemoteResponse)?;
        if mode == CollaborationMode::Plan {
            self.intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?
                .resume_goal = None;
            self.pause_goal_locked().await?;
        }
        let generation = self.current()?;
        let mut snapshot = self
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .settings
            .clone();
        if let Some(model) = &settings.model {
            snapshot["model"] = json!(model);
        }
        if let Some(effort) = &settings.effort {
            snapshot["effort"] = json!(effort);
        }
        let mut params = events::settings(&self.binding.session.id, settings);
        params["collaborationMode"] = goals::mode(&snapshot, mode);
        let expected_mode = params["collaborationMode"]["mode"].clone();
        let prepared = generation.native.prepare(
            "thread/settings/update",
            params,
            generation.permissions.full_access(),
            self.has_history.load(Ordering::Acquire),
            generation.skills.mappings(),
        )?;
        let mut notifications = generation.native.subscribe();
        route::execute(self, &generation, prepared).await?;
        let confirmed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let event = notifications
                    .recv()
                    .await
                    .map_err(|_| ClientError::RemoteResponse)?;
                if event["method"] == "thread/settings/updated"
                    && event["params"]["threadId"] == self.binding.session.id
                    && event["params"]["threadSettings"]["collaborationMode"]["mode"]
                        == expected_mode
                {
                    return Ok::<_, ClientError>(event["params"]["threadSettings"].clone());
                }
            }
        })
        .await
        .map_err(|_| {
            Fault::new(
                "SETTINGS_UNCONFIRMED",
                "Codex did not confirm the mode change.",
            )
        })??;
        self.intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .settings = confirmed.clone();
        let _ = self.events.send(SessionEvent::SettingsChanged {
            settings: remote_codex_adapter::desktop::settings(&confirmed),
        });
        Ok(())
    }
}
