use crate::{
    ClientError, Result,
    store::{CachedSession, LocalStore},
};

pub async fn resolve(store: &LocalStore, id: &str, server: Option<&str>) -> Result<CachedSession> {
    let selected = match server {
        Some(name) => Some(store.find_connection(name).await?.id),
        None => None,
    };
    let mut matches = Vec::new();
    for archived in [false, true] {
        for session in store.cached_sessions(selected.as_deref(), archived).await? {
            if session.session.id == id {
                matches.push(session);
            }
        }
    }
    match matches.len() {
        0 => Err(ClientError::NotFound),
        1 => matches.pop().ok_or(ClientError::NotFound),
        _ => Err(ClientError::Argument(
            "session ID is ambiguous; specify the server with -n",
        )),
    }
}
