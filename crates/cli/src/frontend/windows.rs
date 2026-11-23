use crossterm::{
    event::{self, Event},
    terminal,
};
use portable_pty::{CommandBuilder, PtySize};
use remote_codex_client::{ClientError, Result, application::TerminalAttachment};
use std::io::{Read, Write};
use tokio::sync::mpsc;

struct Killer(Box<dyn portable_pty::ChildKiller + Send + Sync>);
impl Drop for Killer {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}
pub(super) async fn attach(gateway: &mut TerminalAttachment) -> Result<()> {
    let (cols, rows) = terminal::size()?;
    let pair = portable_pty::native_pty_system()
        .openpty(size(cols, rows))
        .map_err(pty_error)?;
    let native = gateway.command.as_std();
    let mut command = CommandBuilder::new(native.get_program());
    command.args(native.get_args());
    if let Some(directory) = native.get_current_dir() {
        command.cwd(directory);
    }
    for (key, value) in native.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    let mut child = pair.slave.spawn_command(command).map_err(pty_error)?;
    drop(pair.slave);
    let mut killer = Killer(child.clone_killer());
    let mut reader = pair.master.try_clone_reader().map_err(pty_error)?;
    let mut input = pair.master.take_writer().map_err(pty_error)?;
    let (output, mut received) = mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        let mut buffer = [0; 16384];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 || output.blocking_send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut output = crate::windows_terminal::Output::default();
    let mut completion = tokio::task::spawn_blocking(move || child.wait());
    let _raw = crate::windows_terminal::RawTerminal::enter()?;
    let mut timer = tokio::time::interval(std::time::Duration::from_millis(16));
    let mut closed = gateway.closed.clone();
    let mut sessions = gateway.current.clone();
    let mut runtime = sessions.borrow_and_update().clone();
    let mut environment = runtime.environment();
    let result = loop {
        tokio::select! {
            status = &mut completion => break status.map_err(pty_error)?.map_err(pty_error),
            chunk = received.recv() => if let Some(bytes) = chunk { output.write(&bytes)?; },
            _ = closed.changed() => { let _ = killer.0.kill(); break Ok(portable_pty::ExitStatus::with_exit_code(0)); },
            changed = sessions.changed() => if changed.is_ok() { runtime = sessions.borrow_and_update().clone(); environment = runtime.environment(); },
            changed = environment.changed() => if changed.is_ok() {
                let state = environment.borrow_and_update().clone();
                if matches!(state, remote_codex_client::session::EnvironmentState::ActionRequired { ref code, .. } if code == "SSH_AUTH_REQUIRED") {
                    terminal::disable_raw_mode()?;
                    let result = runtime.authenticate().await;
                    terminal::enable_raw_mode()?;
                    if let Err(error) = result { eprintln!("{error}"); }
                }
            },
            _ = timer.tick() => while event::poll(std::time::Duration::ZERO)? {
                match event::read()? {
                    Event::Resize(cols, rows) => pair.master.resize(size(cols, rows)).map_err(pty_error)?,
                    Event::Key(key) if key.kind != event::KeyEventKind::Release => { input.write_all(&crate::windows_terminal::key_bytes(key))?; input.flush()?; },
                    Event::Paste(text) => { input.write_all(format!("\x1b[200~{text}\x1b[201~").as_bytes())?; input.flush()?; },
                    _ => {},
                }
            },
        }
    };
    let _ = killer.0.kill();
    // The process can exit before its final terminal cleanup bytes leave ConPTY.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
    while let Ok(Some(bytes)) = tokio::time::timeout_at(deadline, received.recv()).await {
        output.write(&bytes)?;
    }
    let status = result?;
    if !status.success() {
        return Err(ClientError::CodexExit(
            u8::try_from(status.exit_code()).unwrap_or(1),
        ));
    }
    Ok(())
}
fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols,
        rows,
        pixel_width: 0,
        pixel_height: 0,
    }
}
fn pty_error(error: impl std::fmt::Display) -> ClientError {
    std::io::Error::other(error.to_string()).into()
}
