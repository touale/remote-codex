use crate::{args::Cli, progress::PrepareProgress, ui};
use remote_codex_client::{
    Result,
    remote::Remote,
    servers::{self, ServerBundle},
    store::{ConnectionRecord, LocalStore},
};
use std::{
    cell::RefCell,
    path::Path,
    time::{Duration, Instant},
};

pub(crate) mod bundled {
    include!(concat!(env!("OUT_DIR"), "/bundled.rs"));
}

pub(crate) async fn connect(
    cli: &Cli,
    store: &LocalStore,
    directory: &Path,
    record: ConnectionRecord,
) -> Result<Remote> {
    let display = RefCell::new(PrepareProgress::stderr(cli.json));
    let operation = servers::ensure(
        store,
        directory,
        record,
        cli.ssh_config.clone(),
        ui::interactive(),
        ServerBundle {
            bytes: bundled::SERVICE,
            sha256: bundled::SERVICE_SHA,
        },
        |event| display.borrow_mut().event(event, Instant::now()),
    );
    tokio::pin!(operation);
    let mut timer = tokio::time::interval(Duration::from_millis(200));
    let result = loop {
        tokio::select! { result=&mut operation=>break result,_=timer.tick()=>display.borrow_mut().tick(Instant::now()) }
    };
    display.borrow_mut().finish(Instant::now());
    result
}
