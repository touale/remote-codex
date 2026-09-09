use nix::{
    sys::{
        signal::{Signal, kill},
        termios::{self, SetArg, Termios},
    },
    unistd::Pid,
};
use remote_codex_client::Result;

pub(super) struct Terminal(Option<Termios>);

impl Terminal {
    pub(super) fn capture() -> Self {
        Self(termios::tcgetattr(std::io::stdin()).ok())
    }

    pub(super) fn suspend(&self, pid: u32) -> Result<Option<Suspended>> {
        let Some(canonical) = &self.0 else {
            return Ok(None);
        };
        let Ok(pid) = i32::try_from(pid) else {
            return Ok(None);
        };
        let pid = Pid::from_raw(pid);
        let raw = termios::tcgetattr(std::io::stdin()).map_err(std::io::Error::from)?;
        kill(pid, Signal::SIGSTOP).map_err(std::io::Error::from)?;
        let suspended = Suspended { pid, raw };
        termios::tcsetattr(std::io::stdin(), SetArg::TCSANOW, canonical)
            .map_err(std::io::Error::from)?;
        Ok(Some(suspended))
    }
}

pub(super) struct Suspended {
    pid: Pid,
    raw: Termios,
}

impl Drop for Suspended {
    fn drop(&mut self) {
        let _ = termios::tcsetattr(std::io::stdin(), SetArg::TCSANOW, &self.raw);
        let _ = kill(self.pid, Signal::SIGCONT);
        let _ = kill(self.pid, Signal::SIGWINCH);
    }
}
