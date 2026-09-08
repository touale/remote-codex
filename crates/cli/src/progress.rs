mod format;

use remote_codex_client::progress::{PrepareEvent, PrepareStage, TransferProgress};
use std::{
    io::{self, IsTerminal, Write},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Terminal,
    Log,
    Quiet,
}

/// A bounded display of the latest event. Rendering is driven by the CLI timer,
/// so slow transfers and remote checks remain visibly active without event queues.
pub(crate) struct PrepareProgress<W: Write> {
    writer: W,
    mode: Mode,
    stage: Option<PrepareStage>,
    started: Instant,
    transfer_started: Option<Instant>,
    transfer: Option<TransferProgress>,
    service_wait: Option<remote_codex_client::protocol::ServiceActivity>,
    last_draw: Instant,
    line_active: bool,
    terminal_columns: Option<fn() -> usize>,
}

impl PrepareProgress<io::Stderr> {
    pub(crate) fn stderr(json: bool) -> Self {
        let writer = io::stderr();
        let mode = if json {
            Mode::Quiet
        } else if writer.is_terminal() && std::env::var_os("TERM").is_none_or(|v| v != "dumb") {
            Mode::Terminal
        } else {
            Mode::Log
        };
        let mut display = Self::new(writer, mode, Instant::now());
        if mode == Mode::Terminal {
            display.terminal_columns = Some(|| usize::from(console::Term::stderr().size().1));
        }
        display
    }
}

impl<W: Write> PrepareProgress<W> {
    fn new(writer: W, mode: Mode, now: Instant) -> Self {
        Self {
            writer,
            mode,
            stage: None,
            started: now,
            transfer_started: None,
            transfer: None,
            service_wait: None,
            last_draw: now,
            line_active: false,
            terminal_columns: None,
        }
    }

    pub(crate) fn event(&mut self, event: PrepareEvent, now: Instant) {
        if self.mode == Mode::Quiet {
            return;
        }
        match event {
            PrepareEvent::Stage(stage) => {
                self.finish(now);
                self.stage = Some(stage);
                self.started = now;
                self.transfer_started = None;
                self.transfer = None;
                self.draw(now, false);
            }
            PrepareEvent::Transfer(transfer) => {
                let first = self.transfer_started.is_none();
                self.transfer_started.get_or_insert(now);
                self.transfer = Some(transfer);
                if first {
                    self.draw(now, false);
                }
            }
            PrepareEvent::ServiceWaiting(activity) => {
                if self.stage != Some(PrepareStage::WaitService) {
                    self.finish(now);
                    self.stage = Some(PrepareStage::WaitService);
                    self.started = now;
                    self.service_wait = Some(activity);
                    self.draw(now, false);
                } else {
                    self.service_wait = Some(activity);
                }
            }
        }
    }

    pub(crate) fn tick(&mut self, now: Instant) {
        let interval = match self.mode {
            Mode::Terminal => Duration::from_millis(200),
            Mode::Log => Duration::from_secs(5),
            Mode::Quiet => return,
        };
        // OpenSSH owns authentication prompts; never repaint over password/host-key input.
        if self.is_active() && now.duration_since(self.last_draw) >= interval {
            self.draw(now, false);
        }
    }

    pub(crate) fn finish(&mut self, now: Instant) {
        if self.line_active || (self.mode == Mode::Log && self.transfer.is_some()) {
            self.draw(now, true);
        }
        self.stage = None;
        self.transfer = None;
        self.service_wait = None;
        self.transfer_started = None;
    }

    fn is_active(&self) -> bool {
        !matches!(
            self.stage,
            None | Some(
                PrepareStage::ConnectSsh | PrepareStage::UseCachedPackage | PrepareStage::Prepared
            )
        )
    }

    fn draw(&mut self, now: Instant, final_line: bool) {
        let Some(stage) = self.stage else { return };
        if self.mode == Mode::Quiet {
            return;
        }
        let elapsed = now.duration_since(self.started);
        let message = if let Some(activity) = self.service_wait {
            let reason = if activity.jobs > 0 {
                "Waiting for running commands to finish"
            } else {
                "Waiting for the remote environment to become idle"
            };
            format!("{reason} ({}s)", elapsed.as_secs())
        } else {
            match (self.transfer, self.transfer_started) {
                (Some(transfer), Some(started)) => {
                    format::transfer(transfer, now.duration_since(started))
                }
                _ => {
                    let message = format::stage(stage);
                    if self.is_active() {
                        format!("{message} ({}s)", elapsed.as_secs())
                    } else {
                        message.to_owned()
                    }
                }
            }
        };
        let animated = self.mode == Mode::Terminal && self.is_active() && !final_line;
        let result = (|| -> io::Result<()> {
            if self.line_active {
                write!(self.writer, "\r\x1b[2K")?;
            }
            if animated {
                let frame = ["|", "/", "-", "\\"][(elapsed.as_millis() / 200 % 4) as usize];
                let line = format!("remote-codex: {frame} {message}");
                let columns = self.terminal_columns.map_or(usize::MAX, |size| size());
                // Leave the last column unused so terminal auto-wrap cannot
                // strand old spinner frames above the line being cleared.
                write!(
                    self.writer,
                    "{}",
                    console::truncate_str(&line, columns.saturating_sub(1), "…")
                )?;
            } else {
                writeln!(self.writer, "remote-codex: {message}")?;
            }
            self.writer.flush()
        })();
        self.line_active = animated && result.is_ok();
        self.last_draw = now;
        if result.is_err() {
            // A closed progress destination must not fail installation.
            self.mode = Mode::Quiet;
        }
    }
}

impl<W: Write> Drop for PrepareProgress<W> {
    fn drop(&mut self) {
        if self.line_active {
            let _ = writeln!(self.writer);
            let _ = self.writer.flush();
        }
    }
}

#[cfg(test)]
mod tests;
