use crate::Result;
use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixListener,
};
use zeroize::Zeroizing;

pub(super) struct Broker(tokio::task::JoinHandle<()>);

impl Broker {
    pub(super) fn task(task: tokio::task::JoinHandle<()>) -> Self {
        Self(task)
    }
    pub(super) fn start(path: &Path, token: String, password: Zeroizing<String>) -> Result<Self> {
        let listener = UnixListener::bind(path)?;
        Ok(Self(tokio::spawn(async move {
            let deliver = async {
                let (mut stream, _) = listener.accept().await?;
                if stream.peer_cred()?.uid() != nix::unistd::geteuid().as_raw() {
                    return std::io::Result::Ok(());
                }
                let mut request = String::new();
                BufReader::new((&mut stream).take(64))
                    .read_line(&mut request)
                    .await?;
                if request.strip_suffix('\n') == Some(token.as_str()) {
                    stream.write_all(password.as_bytes()).await?;
                    stream.write_all(b"\n").await?;
                }
                stream.shutdown().await
            };
            let _ = tokio::time::timeout(Duration::from_secs(120), deliver).await;
        })))
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        self.0.abort();
    }
}
