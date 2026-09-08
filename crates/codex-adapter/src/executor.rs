//! Native exec-server transport. No model, account, or conversation API is used.
use remote_codex_protocol::{Fault, codec};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, process::Stdio};
use tokio::{
    io::BufReader,
    process::{Child, ChildStdin},
    sync::mpsc,
};

pub struct Executor {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Result<Value, Fault>>,
    reader: tokio::task::JoinHandle<()>,
}

impl Executor {
    pub async fn start(
        program: &Path,
        home: &Path,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, Fault> {
        let mut command = tokio::process::Command::new(program);
        command.env_clear();
        for name in ["PATH", "HOME", "USER", "LOGNAME", "SHELL", "LANG", "TMPDIR"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command
            .envs(env)
            .env("CODEX_HOME", home)
            .args(["exec-server", "--listen", "stdio"])
            .current_dir(home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| {
            Fault::new(
                "EXECUTOR_START_FAILED",
                "cannot start native execution backend",
            )
        })?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| Fault::new("EXECUTOR_START_FAILED", "executor input unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Fault::new("EXECUTOR_START_FAILED", "executor output unavailable"))?;
        let (send, output) = mpsc::channel(64);
        let reader = tokio::spawn(async move {
            let mut input = codec::Reader::new(BufReader::new(stdout));
            loop {
                match input.next::<Value>().await {
                    Ok(Some(value)) => {
                        if send.send(Ok(value)).await.is_err() {
                            break;
                        }
                    }
                    _ => {
                        let _ = send
                            .send(Err(Fault::unknown("native executor stopped")))
                            .await;
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            input,
            output,
            reader,
        })
    }
    pub async fn send(&mut self, value: &Value) -> Result<(), Fault> {
        codec::write(&mut self.input, value)
            .await
            .map_err(|_| Fault::unknown("native execution transport closed"))
    }
    pub async fn next(&mut self) -> Option<Result<Value, Fault>> {
        self.output.recv().await
    }
    pub async fn shutdown(&mut self) {
        self.reader.abort();
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.reader.abort();
        if let Some(id) = self.child.id().and_then(|p| i32::try_from(p).ok()) {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(id),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
}
