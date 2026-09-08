use sqlx::SqliteConnection;

use super::{ConfigItem, ConfigReport, ConfigSnapshot, LocalStore, Revision, codec};
use crate::{
    ClientError, Result,
    config::{ConfigInput, ConfigLayer, Source, resolve},
};

impl LocalStore {
    pub(crate) async fn set_many_config(
        &self,
        server: &str,
        changes: &[(String, String)],
        expected: Revision,
    ) -> Result<Revision> {
        let mut tx = self.begin_write().await?;
        let mut state = snapshot(&mut tx, server).await?;
        if state.revision != expected {
            return Err(ClientError::RevisionConflict);
        }
        let before = state.layer.clone();
        for (key, value) in changes {
            state.layer.set(key, ConfigInput::Plain(value))?;
        }
        let revision = replace(&mut tx, server, &before, &state.layer).await?;
        tx.commit().await?;
        Ok(revision)
    }

    pub(crate) async fn config_snapshot(&self, server: &str) -> Result<ConfigSnapshot> {
        let mut tx = self.pool.begin().await?;
        let state = snapshot(&mut tx, server).await?;
        tx.commit().await?;
        Ok(state)
    }

    pub(crate) async fn config_report(
        &self,
        server: &str,
        overrides: bool,
    ) -> Result<ConfigReport> {
        let state = self.config_snapshot(server).await?;
        let applied_revision = self.server_access(server).await?.applied_revision;
        let mut items: Vec<ConfigItem> = state
            .effective
            .list()
            .into_iter()
            .filter(|item| !overrides || item.source == Source::Server)
            .map(|setting| ConfigItem {
                application_state: if applied_revision == Some(state.revision.saved) {
                    match setting.apply_policy {
                        crate::config::ApplyPolicy::NewProcess => "new_sessions",
                        _ => "applied",
                    }
                } else {
                    "pending_sync"
                },
                setting,
            })
            .collect();
        if !overrides {
            use crate::config::{ApplyPolicy, SettingView, VisibleValue};
            let record = self.connection_by_id(server).await?;
            let identity = [
                (
                    "ssh.host",
                    Some(VisibleValue::Text(record.endpoint.host().to_owned())),
                ),
                (
                    "ssh.user",
                    record
                        .endpoint
                        .user()
                        .map(|value| VisibleValue::Text(value.to_owned())),
                ),
                (
                    "ssh.port",
                    record.endpoint.port().map(VisibleValue::Integer),
                ),
            ];
            for (key, value) in identity {
                items.push(ConfigItem {
                    setting: SettingView {
                        key: key.to_owned(),
                        value,
                        redacted: false,
                        source: Source::Server,
                        apply_policy: ApplyPolicy::ReadOnly,
                        masked_by_proxy_mode: false,
                    },
                    application_state: "identity",
                });
            }
        }
        Ok(ConfigReport {
            server_id: server.to_owned(),
            saved_revision: state.revision.saved,
            applied_revision,
            items,
        })
    }

    pub(crate) async fn set_config(
        &self,
        server: &str,
        key: &str,
        input: ConfigInput<'_>,
        expected: Revision,
    ) -> Result<Revision> {
        let mut tx = self.begin_write().await?;
        let mut state = snapshot(&mut tx, server).await?;
        if state.revision != expected {
            return Err(ClientError::RevisionConflict);
        }
        let before = state.layer.clone();
        state.layer.set(key, input)?;
        let revision = replace(&mut tx, server, &before, &state.layer).await?;
        tx.commit().await?;
        Ok(revision)
    }

    pub(crate) async fn unset_config(
        &self,
        server: &str,
        key: &str,
        expected: Revision,
    ) -> Result<Revision> {
        let mut tx = self.begin_write().await?;
        let mut state = snapshot(&mut tx, server).await?;
        if state.revision != expected {
            return Err(ClientError::RevisionConflict);
        }
        let before = state.layer.clone();
        state.layer.unset(key)?;
        let revision = replace(&mut tx, server, &before, &state.layer).await?;
        tx.commit().await?;
        Ok(revision)
    }
}

async fn revision(connection: &mut SqliteConnection, server: &str) -> Result<Revision> {
    let saved = sqlx::query_scalar(
        "SELECT r.revision FROM server_revisions r JOIN connections c ON c.id=r.id WHERE c.id=?",
    )
    .bind(server)
    .fetch_optional(connection)
    .await?
    .ok_or(ClientError::NotFound)?;
    Ok(Revision { saved })
}

async fn snapshot(connection: &mut SqliteConnection, server: &str) -> Result<ConfigSnapshot> {
    let revision = revision(connection, server).await?;
    let layer = codec::load(connection, server).await?;
    let effective = resolve(&layer)?;
    Ok(ConfigSnapshot {
        revision,
        layer,
        effective,
    })
}

async fn replace(
    connection: &mut SqliteConnection,
    server: &str,
    before: &ConfigLayer,
    after: &ConfigLayer,
) -> Result<Revision> {
    if before == after {
        return revision(connection, server).await;
    }
    resolve(after)?;
    codec::save(connection, server, after).await?;
    let count = sqlx::query(
        "UPDATE server_revisions SET revision=revision+1 WHERE id=? AND revision<9223372036854775807",
    ).bind(server).execute(&mut *connection).await?.rows_affected();
    if count != 1 {
        return Err(ClientError::RevisionConflict);
    }
    revision(connection, server).await
}
