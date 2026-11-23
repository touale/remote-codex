use super::ShellEvent;
use crate::{ClientError, Result, remote::Remote};
use russh::ChannelMsg;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::mpsc;

enum Control {
    Input(Vec<u8>),
    Resize(u16, u16),
}
pub struct ShellHandle {
    cancel: tokio::sync::watch::Sender<bool>,
    control: mpsc::Sender<Control>,
    events: tokio::sync::Mutex<mpsc::Receiver<ShellEvent>>,
    closed: Arc<AtomicBool>,
    path: String,
    remote: Arc<Remote>,
}
pub(in crate::application) async fn open(
    remote: Arc<Remote>,
    path: &str,
    columns: u16,
    rows: u16,
    workspace_lock: crate::workspace_lock::WorkspaceLock,
) -> Result<ShellHandle> {
    dimensions(columns, rows)?;
    let script = format!(
        "cd -- {} && exec \"${{SHELL:-/bin/sh}}\" -l",
        crate::ssh::quote(path)?
    );
    let (mut reader, writer) = remote.ssh.pty(&script, columns, rows).await?.split();
    let (cancel, mut cancelled) = tokio::sync::watch::channel(false);
    let (control, mut controls) = mpsc::channel(64);
    let (sender, events) = mpsc::channel(64);
    let closed = Arc::new(AtomicBool::new(false));
    let finished = closed.clone();
    tokio::spawn(async move {
        let _workspace_lock = workspace_lock;
        let work = async {
            let mut code = None;
            loop {
                tokio::select! {
                    command = controls.recv() => match command {
                        Some(Control::Input(bytes)) => { if writer.data(&bytes[..]).await.is_err() { break; } },
                        Some(Control::Resize(cols, rows)) => { let _ = writer.window_change(u32::from(cols), u32::from(rows), 0, 0).await; },
                        None => break,
                    },
                    message = reader.wait() => match message {
                        Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                            if sender.send(ShellEvent::Output { bytes: data.to_vec() }).await.is_err() { break; }
                        },
                        Some(ChannelMsg::ExitStatus { exit_status }) => code = Some(exit_status),
                        Some(ChannelMsg::Close) | None => break,
                        _ => {},
                    },
                }
            }
            code
        };
        let code = tokio::select! { code = work => code, _ = cancelled.changed() => None };
        let _ = writer.close().await;

        finished.store(true, Ordering::Release);
        let _ = sender.send(ShellEvent::Closed { code }).await;
    });
    Ok(ShellHandle {
        cancel,
        control,
        events: tokio::sync::Mutex::new(events),
        closed,
        path: path.into(),
        remote,
    })
}
impl ShellHandle {
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    pub fn initial_path(&self) -> &str {
        &self.path
    }
    pub fn server(&self) -> &str {
        &self.remote.server.name
    }
    pub async fn next(&self) -> Option<ShellEvent> {
        self.events.lock().await.recv().await
    }
    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > 65536 || self.is_closed() {
            return Err(ClientError::Argument(
                "Terminal is closed or input is too large.",
            ));
        }
        self.control
            .try_send(Control::Input(bytes.to_vec()))
            .map_err(|_| ClientError::Argument("Terminal input is busy; retry."))
    }
    pub fn resize(&self, columns: u16, rows: u16) -> Result<()> {
        dimensions(columns, rows)?;
        self.control
            .try_send(Control::Resize(columns, rows))
            .map_err(|_| ClientError::Argument("Terminal is busy; retry resizing."))
    }
    pub fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.cancel.send_replace(true);
        }
    }
}
impl Drop for ShellHandle {
    fn drop(&mut self) {
        self.close();
    }
}
fn dimensions(columns: u16, rows: u16) -> Result<()> {
    if !(2..=500).contains(&columns) || !(2..=300).contains(&rows) {
        return Err(ClientError::Argument("Invalid terminal dimensions."));
    }
    Ok(())
}
