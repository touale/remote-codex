use crate::{ClientError, Result, remote::Remote, store::LocalStore};
use futures_util::{StreamExt, stream};

use serde::Serialize;
use std::{path::PathBuf, time::Duration};

#[derive(Debug, Serialize)]
pub struct ServerStatus {
    pub server: String,
    pub status: String,
    pub error: Option<String>,
    pub checked_at: i64,
}

pub async fn refresh(
    store: &LocalStore,
    selected: Option<&str>,
    ssh_config: Option<PathBuf>,
) -> Result<Vec<ServerStatus>> {
    let records = match selected {
        Some(name) => vec![store.find_connection(name).await?],
        None => store.list_connections().await?,
    };
    let checks = stream::iter(records)
        .map(|record| {
            let ssh_config = ssh_config.clone();
            async move {
                let name = record.name.clone();
                let server_id = record.id.clone();
                let operation = async {
                    let remote = Remote::connect_running(store, record, ssh_config, true).await?;
                    remote.synchronize(store).await?;
                    remote.close().await?;
                    Ok::<_, ClientError>(())
                };
                let result = tokio::time::timeout(Duration::from_secs(10), operation).await;
                let checked_at = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let result = match result {
                    Ok(Ok(())) => ServerStatus {
                        server: name,
                        status: "ready".into(),
                        error: None,
                        checked_at,
                    },
                    Ok(Err(error)) => ServerStatus {
                        server: name,
                        status: "unavailable".into(),
                        error: Some(error.to_string()),
                        checked_at,
                    },
                    Err(_) => ServerStatus {
                        server: name,
                        status: "timeout".into(),
                        error: Some("server check timed out".into()),
                        checked_at,
                    },
                };
                if let Ok(mut access) = store.server_access(&server_id).await {
                    access.checked_at = Some(checked_at);
                    access.health = result.status.clone();
                    let _ = store.save_access(&server_id, &access).await;
                }
                result
            }
        })
        .buffer_unordered(4);
    Ok(checks.collect().await)
}
