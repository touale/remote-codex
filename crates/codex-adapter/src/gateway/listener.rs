#[cfg(unix)]
use super::Fault;
use super::{Result, transport};
#[cfg(unix)]
use std::path::PathBuf;
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(windows)]
use tokio::net::TcpListener as NativeListener;
#[cfg(unix)]
use tokio::net::UnixListener as NativeListener;

pub(super) trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}
pub(super) type Stream = Box<dyn Io>;

pub(super) struct Listener {
    inner: NativeListener,
    pub url: String,
    pub token: Option<String>,
    #[cfg(unix)]
    pub socket: PathBuf,
    #[cfg(unix)]
    _directory: tempfile::TempDir,
}

impl Listener {
    pub(super) async fn bind() -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let directory = tempfile::Builder::new()
                .prefix("rc-ui-")
                .permissions(std::fs::Permissions::from_mode(0o700))
                .tempdir_in("/tmp")
                .map_err(transport)?;
            let socket = directory.path().join("frontend.sock");
            let inner = NativeListener::bind(&socket).map_err(transport)?;
            Ok(Self {
                url: format!("unix://{}", socket.display()),
                token: None,
                socket,
                inner,
                _directory: directory,
            })
        }
        #[cfg(windows)]
        {
            let inner = NativeListener::bind("127.0.0.1:0")
                .await
                .map_err(transport)?;
            let url = format!("ws://{}", inner.local_addr().map_err(transport)?);
            let token = Some(format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ));
            Ok(Self { url, token, inner })
        }
    }

    pub(super) async fn accept(&self) -> Result<Stream> {
        let (stream, _) = self.inner.accept().await.map_err(transport)?;
        #[cfg(unix)]
        if stream.peer_cred().map_err(transport)?.uid() != nix::unistd::geteuid().as_raw() {
            return Err(Fault::new(
                "FRONTEND_OWNER",
                "frontend must belong to the current user",
            ));
        }
        Ok(Box::new(stream))
    }
}
