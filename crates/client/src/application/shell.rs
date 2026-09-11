use crate::{ClientError, Result, remote::Remote};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShellEvent {
    Output { bytes: Vec<u8> },
    Closed { code: Option<u32> },
}

pub struct ShellHandle {
    workspace_lock: crate::workspace_lock::WorkspaceLock,
    master: Mutex<Box<dyn MasterPty + Send>>,
    input: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    events: tokio::sync::Mutex<mpsc::Receiver<ShellEvent>>,
    closed: std::sync::atomic::AtomicBool,
    path: String,
    _remote: Arc<Remote>,
}

pub(super) async fn open(
    remote: Arc<Remote>,
    path: &str,
    columns: u16,
    rows: u16,
    workspace_lock: crate::workspace_lock::WorkspaceLock,
) -> Result<ShellHandle> {
    let script = format!(
        "cd -- {} && exec \"${{SHELL:-/bin/sh}}\" -l",
        crate::ssh::quote(path)?
    );
    let ssh = remote
        .ssh
        .command(&remote.server.endpoint, Some(&script), true);
    let native = ssh.as_std();
    let mut command = CommandBuilder::new(native.get_program());
    command.args(native.get_args());
    command.env("TERM", "xterm-256color");
    let pair = portable_pty::native_pty_system()
        .openpty(size(columns, rows)?)
        .map_err(pty_error)?;
    let child = pair.slave.spawn_command(command).map_err(pty_error)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(pty_error)?;
    let input = pair.master.take_writer().map_err(pty_error)?;
    let (sender, events) = mpsc::channel(64);
    std::thread::spawn(move || {
        let mut buffer = vec![0; 16384];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 {
                break;
            }
            if sender
                .blocking_send(ShellEvent::Output {
                    bytes: buffer[..count].into(),
                })
                .is_err()
            {
                break;
            }
        }
        let _ = sender.blocking_send(ShellEvent::Closed { code: None });
    });
    Ok(ShellHandle {
        workspace_lock,
        master: Mutex::new(pair.master),
        input: Mutex::new(input),
        child: Mutex::new(child),
        events: tokio::sync::Mutex::new(events),
        closed: std::sync::atomic::AtomicBool::new(false),
        path: path.into(),
        _remote: remote,
    })
}

impl ShellHandle {
    pub fn initial_path(&self) -> &str {
        &self.path
    }
    pub fn server(&self) -> &str {
        &self._remote.server.name
    }
    pub async fn next(&self) -> Option<ShellEvent> {
        self.events.lock().await.recv().await
    }
    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > 65536 || self.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ClientError::Argument(
                "Terminal is closed or input is too large.",
            ));
        }
        let mut input = self.input.lock().map_err(|_| ClientError::RemoteResponse)?;
        input.write_all(bytes)?;
        input.flush()?;
        Ok(())
    }
    pub fn resize(&self, columns: u16, rows: u16) -> Result<()> {
        self.master
            .lock()
            .map_err(|_| ClientError::RemoteResponse)?
            .resize(size(columns, rows)?)
            .map_err(pty_error)
    }
    pub fn close(&self) {
        if !self.closed.swap(true, std::sync::atomic::Ordering::AcqRel) {
            if let Ok(mut child) = self.child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
            self.workspace_lock.release();
        }
    }
}
impl Drop for ShellHandle {
    fn drop(&mut self) {
        self.close();
    }
}
fn size(cols: u16, rows: u16) -> Result<PtySize> {
    if !(2..=500).contains(&cols) || !(2..=300).contains(&rows) {
        return Err(ClientError::Argument("Invalid terminal dimensions."));
    }
    Ok(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })
}
fn pty_error(error: impl std::fmt::Display) -> ClientError {
    std::io::Error::other(error.to_string()).into()
}
