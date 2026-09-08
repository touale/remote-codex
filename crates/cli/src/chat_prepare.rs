use crate::{args::Cli, progress::PrepareProgress};
use remote_codex_client::{
    Result,
    local::{LocalRuntime, OpenOptions},
    remote::Remote,
    store::LocalStore,
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(crate) async fn open(
    cli: &Cli,
    store: &LocalStore,
    remote: Arc<Remote>,
    mut options: OpenOptions<'_>,
) -> Result<Arc<LocalRuntime>> {
    let display = Arc::new(Mutex::new(PrepareProgress::stderr(cli.json)));
    let events = display.clone();
    options.progress = Some(Arc::new(move |event| {
        if let Ok(mut display) = events.lock() {
            display.event(event, Instant::now());
        }
    }));
    let operation = LocalRuntime::open(store, remote, options);
    tokio::pin!(operation);
    let mut timer = tokio::time::interval(Duration::from_millis(200));
    let result = loop {
        tokio::select! {result=&mut operation=>break result,_=timer.tick()=>{if let Ok(mut display)=display.lock(){display.tick(Instant::now());}}}
    };
    if let Ok(mut display) = display.lock() {
        display.finish(Instant::now());
    }
    result
}
