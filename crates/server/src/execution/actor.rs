use super::{
    Execution, Input,
    operations::{Pending, collect, dispatch},
};
use crate::{Result, config::Runtime, storage::Store};
use remote_codex_adapter::executor::Executor;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::Weak,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

pub(super) async fn run(
    identity: (&str, &str),
    mut runtime: Runtime,
    mut updates: tokio::sync::watch::Receiver<crate::profiles::LiveSettings>,
    store: Store,
    mut backend: Executor,
    mut input: mpsc::Receiver<Input>,
    handle: &Weak<Execution>,
) {
    let (id, profile) = identity;
    let mut pending = HashMap::<String, Pending>::new();
    let mut touched = Instant::now();
    let mut attached = true;
    let mut detached = false;
    let mut timer = tokio::time::interval(Duration::from_secs(1));
    loop {
        let result: Result<()> = tokio::select! {
            command = input.recv() => match command {
                Some(Input::Execute { operation, message, approval, permissions }) => {
                    touched = Instant::now();
                    dispatch(
                        (id, profile),
                        &runtime,
                        &store,
                        &mut backend,
                        &mut pending,
                        (&operation, approval.as_deref(), permissions.as_deref()),
                        message,
                    ).await
                }
                Some(Input::Touch) => {
                    attached = true;
                    detached = false;
                    touched = Instant::now();
                    Ok(())
                }
                Some(Input::Detached) => {
                    attached = false;
                    detached = true;
                    touched = Instant::now();
                    terminate(&store, &mut backend, id, true).await
                }
                Some(Input::RetireIdle) => {
                    if detached && pending.is_empty()
                        && store.running_processes(id, false).await.is_ok_and(|jobs| jobs.is_empty())
                    {
                        break;
                    }
                    Ok(())
                }
                Some(Input::Disconnected) => {
                    attached = false;
                    touched = Instant::now();
                    Ok(())
                }
                Some(Input::Stop) | None => break,
            },
            output = backend.next() => match output {
                Some(Ok(value)) => collect(id, &store, &mut pending, value).await,
                _ => break,
            },
            update = updates.changed() => {
                if update.is_err() {
                    break;
                }
                let latest = *updates.borrow_and_update();
                runtime.background = latest.background;
                runtime.grace = latest.grace;
                Ok(())
            },
            _ = timer.tick() => {
                if attached && touched.elapsed() > Duration::from_secs(15) {
                    attached = false;
                    touched = Instant::now();
                    let _ = terminate(&store, &mut backend, id, true).await;
                }
                if !attached && !runtime.background
                    && touched.elapsed() > Duration::from_secs(runtime.grace)
                {
                    break;
                }
                if !attached && touched.elapsed() > Duration::from_secs(15) {
                    let _ = terminate(&store, &mut backend, id, true).await;
                }
                if !attached && runtime.background && touched.elapsed() > Duration::from_secs(60)
                    && store.active_jobs(id).await.is_ok_and(|count| count == 0)
                {
                    break;
                }
                Ok(())
            }
        };
        if result.is_err() {
            break;
        }
        if let Some(handle) = handle.upgrade() {
            handle.changed.notify_waiters();
        }
    }
    let _ = terminate(&store, &mut backend, id, false).await;
    let deadline = tokio::time::sleep(Duration::from_secs(2));
    tokio::pin!(deadline);
    loop {
        if store
            .running_processes(id, false)
            .await
            .is_ok_and(|jobs| jobs.is_empty())
        {
            break;
        }
        tokio::select! {
            _ = &mut deadline => break,
            message = backend.next() => match message {
                Some(Ok(value)) => { let _ = collect(id, &store, &mut pending, value).await; }
                _ => break,
            }
        }
    }
    backend.shutdown().await;
    let _ = store.mark_channel_lost(id).await;
}

async fn terminate(
    store: &Store,
    backend: &mut Executor,
    channel: &str,
    mcp_only: bool,
) -> Result<()> {
    for process in store.running_processes(channel, mcp_only).await? {
        backend
            .send(&json!({
                "id": format!("cleanup-{}", uuid::Uuid::new_v4()),
                "method": "process/terminate",
                "params": {"processId": process}
            }))
            .await?;
    }
    Ok(())
}
