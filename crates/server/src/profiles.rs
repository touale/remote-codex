//! Persisted configuration snapshots and live lifecycle policy notifications.
use crate::{Result, config::Runtime, storage::Store};
use remote_codex_protocol::RemoteConfig;
use std::collections::HashMap;
use tokio::sync::{Mutex, watch};

#[derive(Clone, Copy)]
pub(crate) struct LiveSettings {
    pub(crate) background: bool,
    pub(crate) grace: u64,
}

struct Profile {
    runtime: Runtime,
    updates: watch::Sender<LiveSettings>,
}

pub(crate) struct Profiles {
    store: Store,
    profiles: Mutex<HashMap<String, Profile>>,
}

impl Profiles {
    pub(crate) fn new(store: Store) -> Self {
        Self {
            store,
            profiles: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn configure(&self, id: &str, config: RemoteConfig) -> Result<()> {
        let runtime = Runtime::load(config.clone()).await?;
        // Serialize persistence and publication so concurrent revisions cannot
        // publish an older snapshot after a newer database commit.
        let mut profiles = self.profiles.lock().await;
        self.store.save_config(id, &config).await?;
        if let Some(profile) = profiles.get_mut(id) {
            profile.updates.send_replace(settings(&runtime));
            profile.runtime = runtime;
        }
        Ok(())
    }

    pub(crate) async fn snapshot(
        &self,
        id: &str,
    ) -> Result<(Runtime, watch::Receiver<LiveSettings>)> {
        let mut profiles = self.profiles.lock().await;
        profiles.retain(|_, profile| profile.updates.receiver_count() > 0);
        if !profiles.contains_key(id) {
            let runtime = Runtime::load(self.store.config(id).await?).await?;
            let updates = watch::channel(settings(&runtime)).0;
            profiles.insert(id.into(), Profile { runtime, updates });
        }
        let profile = &profiles[id];
        Ok((profile.runtime.clone(), profile.updates.subscribe()))
    }
}

fn settings(runtime: &Runtime) -> LiveSettings {
    LiveSettings {
        background: runtime.background,
        grace: runtime.grace,
    }
}
