use crate::{
    context::Context,
    windows_terminal::{RawTerminal, key_bytes},
};
use crossterm::{
    event::{self, Event},
    terminal,
};
use remote_codex_client::{Result, application::ShellEvent};

pub(crate) async fn run(context: &Context, server: &str, path: Option<&str>) -> Result<()> {
    let (cols, rows) = terminal::size()?;
    let remote = match path {
        Some(path) => {
            context
                .client
                .servers()
                .files(server, path)
                .await?
                .terminal(cols, rows)
                .await?
        }
        None => {
            context
                .client
                .servers()
                .terminal(server, cols, rows)
                .await?
        }
    };
    let mut output = crate::windows_terminal::Output::default();
    let _raw = RawTerminal::enter()?;
    let mut timer = tokio::time::interval(std::time::Duration::from_millis(16));
    loop {
        tokio::select! {
            event = remote.next() => match event {
                Some(ShellEvent::Output { bytes }) => { output.write(&bytes)?; },
                Some(ShellEvent::Closed { .. }) | None => break,
            },
            _ = timer.tick() => while event::poll(std::time::Duration::ZERO)? {
                match event::read()? {
                    Event::Resize(cols, rows) => remote.resize(cols, rows)?,
                    Event::Key(key) if key.kind != event::KeyEventKind::Release => remote.write(&key_bytes(key))?,
                    Event::Paste(text) => remote.write(format!("\x1b[200~{text}\x1b[201~").as_bytes())?,
                    _ => {},
                }
            },
        }
    }
    remote.close();
    Ok(())
}
