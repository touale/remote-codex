//! Bounded PTY driver; API events and persisted results verify task state.
use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::ProbeResult;

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

pub struct Terminal {
    child: Box<dyn Child + Send + Sync>,
    _master: Box<dyn MasterPty + Send>,
    writer: Writer,
    tail: Arc<Mutex<VecDeque<u8>>>,
}

impl Terminal {
    pub fn spawn(command: CommandBuilder) -> ProbeResult<Self> {
        let pair = native_pty_system().openpty(PtySize {
            rows: 32,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let reader = pair.master.try_clone_reader()?;
        let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let terminal_writer = writer.clone();
        let terminal_tail = tail.clone();
        std::thread::spawn(move || drain(reader, terminal_writer, terminal_tail));
        Ok(Self {
            child,
            _master: pair.master,
            writer,
            tail,
        })
    }

    pub fn process_id(&self) -> ProbeResult<u32> {
        self.child
            .process_id()
            .ok_or_else(|| "terminal child PID unavailable".into())
    }

    pub fn send(&self, input: &[u8]) -> ProbeResult<()> {
        let mut writer = self.writer.lock().map_err(|_| "terminal writer poisoned")?;
        writer
            .write_all(input)
            .and_then(|()| writer.flush())
            .map_err(|error| format!("terminal input failed: {error}: {}", self.diagnostics()))?;
        Ok(())
    }

    pub async fn type_command(&self, command: &str) -> ProbeResult<()> {
        for byte in command.bytes() {
            self.send(&[byte])?;
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        // Flush the terminal paste-burst detector before pressing Enter.
        tokio::time::sleep(Duration::from_millis(250)).await;
        self.send(b"\r")
    }

    pub async fn wait_for(&self, text: &str) -> ProbeResult<()> {
        tokio::time::timeout(Duration::from_secs(45), async {
            while !self.diagnostics().contains(text) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| format!("terminal did not show {text}: {}", self.diagnostics()))?;
        Ok(())
    }

    pub async fn wait_exit(&mut self) -> ProbeResult<Option<u32>> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Ok(Some(status.exit_code()));
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(None);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    pub async fn interrupt(&mut self) -> ProbeResult<Option<u32>> {
        self.send(b"\x03")?;
        tokio::time::sleep(Duration::from_millis(350)).await;
        // Native remote frontends may exit on the first Ctrl+C.
        if let Some(status) = self.child.try_wait()? {
            return Ok(Some(status.exit_code()));
        }
        self.send(b"\x03")?;
        self.wait_exit().await
    }

    pub fn diagnostics(&self) -> String {
        self.tail
            .lock()
            .map(|tail| {
                String::from_utf8_lossy(&tail.iter().copied().collect::<Vec<_>>()).into_owned()
            })
            .unwrap_or_default()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        // The reader ends at PTY EOF/EIO after the child and master close.
    }
}

fn drain(mut reader: Box<dyn Read + Send>, writer: Writer, tail: Arc<Mutex<VecDeque<u8>>>) {
    let mut buffer = [0u8; 4096];
    let mut pending = Vec::new();
    while let Ok(read) = reader.read(&mut buffer) {
        if read == 0 {
            break;
        }
        if let Ok(mut tail) = tail.lock() {
            for byte in &buffer[..read] {
                if tail.len() >= 256 * 1024 {
                    tail.pop_front();
                }
                tail.push_back(*byte);
            }
        }
        pending.extend_from_slice(&buffer[..read]);
        for (query, reply) in [
            (b"\x1b[6n".as_slice(), b"\x1b[1;1R".as_slice()),
            (b"\x1b[?u".as_slice(), b"\x1b[?0u".as_slice()),
            (b"\x1b[c".as_slice(), b"\x1b[?1;2c".as_slice()),
            (
                b"\x1b]10;?\x07".as_slice(),
                b"\x1b]10;rgb:ffff/ffff/ffff\x07".as_slice(),
            ),
            (
                b"\x1b]11;?\x07".as_slice(),
                b"\x1b]11;rgb:0000/0000/0000\x07".as_slice(),
            ),
        ] {
            while let Some(index) = pending
                .windows(query.len())
                .position(|window| window == query)
            {
                pending.drain(index..index + query.len());
                if let Ok(mut writer) = writer.lock() {
                    let _ = writer.write_all(reply);
                    let _ = writer.flush();
                }
            }
        }
        if pending.len() > 16 {
            pending.drain(..pending.len() - 16);
        }
    }
}
