use crate::{
    args::{Cli, Command, ServerCommand},
    progress::PrepareProgress,
    ui,
};
use remote_codex_client::{
    Result,
    application::{Client, ClientOptions, ServiceBundle},
};
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

mod bundled {
    include!(concat!(env!("OUT_DIR"), "/bundled.rs"));
}

pub(crate) struct Context {
    pub(crate) client: Client,
    display: Arc<Mutex<PrepareProgress<std::io::Stderr>>>,
}

impl Context {
    pub(crate) async fn open(cli: &Cli) -> Result<Self> {
        let display = Arc::new(Mutex::new(PrepareProgress::stderr(cli.json)));
        let events = display.clone();
        let notices = display.clone();
        let json = cli.json;
        let client = Client::open(ClientOptions {
            data_dir: cli.data_dir.clone(),
            ssh_config: cli.ssh_config.clone(),
            interactive: ui::interactive()
                && !cli.json
                && !matches!(&cli.command,
                Some(Command::Server {command: ServerCommand::Add(args)}) if args.non_interactive),
            service: ServiceBundle {
                bytes: Arc::from(bundled::SERVICE),
                sha256: bundled::SERVICE_SHA.into(),
            },
            progress: Some(Arc::new(move |event| {
                if let Ok(mut display) = events.lock() {
                    display.event(event, Instant::now());
                }
            })),
            notice: Some(Arc::new(move |notice| {
                if let Ok(mut display) = notices.lock() {
                    display.finish(Instant::now());
                    let _ = crate::output::notice(&mut std::io::stderr(), notice, json);
                }
            })),
        })
        .await?;
        Ok(Self { client, display })
    }

    pub(crate) async fn prepare<T>(&self, operation: impl Future<Output = Result<T>>) -> Result<T> {
        tokio::pin!(operation);
        let mut timer = tokio::time::interval(Duration::from_millis(200));
        let result = loop {
            tokio::select! {
                result = &mut operation => break result,
                _ = timer.tick() => { if let Ok(mut display) = self.display.lock() { display.tick(Instant::now()); } }
            }
        };
        if let Ok(mut display) = self.display.lock() {
            display.finish(Instant::now());
        }
        result
    }
}
