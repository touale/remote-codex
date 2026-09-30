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
        let goal = intent.goal.clone();
        drop(intent);
        let _ = self
            .events
            .send(SessionEvent::GoalChanged { goal: goal.clone() });
        Ok(goal)
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
            self.has_history.load(Ordering::Acquire),
            generation.skills.mappings(),
        )?;
        route::execute(self, &generation, prepared).await?;
        self.load_goal().await
    }
    pub(crate) async fn goal(&self, action: GoalAction) -> Result<Option<Goal>> {
        let _gate = self.goal_gate.lock().await;
        if matches!(action, GoalAction::Set { .. }) {
            self.idle()?;
        }
        if matches!(action, GoalAction::Pause | GoalAction::Clear) {
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
        let replaces_goal = set || matches!(action, GoalAction::Resume);
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
        let goal = self.goal_request(method, params).await?;
        if replaces_goal {
            self.intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?
                .resume_goal = None;
        }
        Ok(goal)
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
        let objective = {
            let intent = self
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            if intent.interrupted.is_some()
                || intent.episode.is_some()
                || intent.pending_message.is_some()
            {
                return Ok(());
            }
            let Some(objective) = intent.resume_goal.clone() else {
                return Ok(());
            };
            objective
        };
        let paused = |goal: &Option<Goal>| {
            goal.as_ref()
                .is_some_and(|g| g.objective == objective && g.status == GoalStatus::Paused)
        };
        // A previous activation may have succeeded without its acknowledgement.
        let goal = self.load_goal().await?;
        {
            let mut intent = self
                .intent
                .lock()
                .map_err(|_| ClientError::RemoteResponse)?;
            if intent.resume_goal.as_ref() != Some(&objective)
                || intent.interrupted.is_some()
                || intent.episode.is_some()
                || intent.pending_message.is_some()
            {
                return Ok(());
            }
            if !paused(&goal) {
                intent.resume_goal = None;
                return Ok(());
            }
        }
        let (method, params) = goals::params(&self.binding.session.id, GoalAction::Resume)?;
        if let Err(error) = self.goal_request(method, params).await {
            if !error.outcome_is_unknown() || self.recovery.ready().is_err() {
                return Err(error);
            }
            match self.load_goal().await {
                Ok(goal) if !paused(&goal) => {}
                _ => return Err(error),
            }
        }
        let mut intent = self
            .intent
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?;
        if intent.resume_goal.as_ref() == Some(&objective) {
            intent.resume_goal = None;
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
