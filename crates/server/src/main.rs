use clap::{Parser, Subcommand};
use remote_codex_protocol::Fault;
use remote_codex_server::{Result, paths, service::Service};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    net::{UnixListener, UnixStream},
    process::Command,
};

#[derive(Parser)]
#[command(version)]
struct Args {
    #[arg(long)]
    state: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}

#[derive(Subcommand)]
enum Action {
    Serve,
    Ensure {
        #[arg(long)]
        json: bool,
    },
    Relay,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let json = matches!(args.command, Action::Ensure { json: true });
    match run(args).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            if json {
                print_status(&remote_codex_protocol::ServiceStart::Failed(error));
            } else {
                eprintln!("remote-codex-server: {} [{}]", error.message, error.code);
            }
            std::process::ExitCode::from(7)
        }
    }
}

async fn run(args: Args) -> Result<()> {
    let root = match args.state {
        Some(path) => path,
        None => PathBuf::from(
            std::env::var_os("HOME").ok_or_else(|| fault("SSH user home is unavailable"))?,
        )
        .join(".local/share/remote-codex"),
    };
    paths::private(&root)?;
    let socket = paths::socket(&root)?;
    match args.command {
        Action::Serve => {
            let lock = paths::file(&root.join("service.lock"))?;
            let _lock = nix::fcntl::Flock::lock(lock, nix::fcntl::FlockArg::LockExclusiveNonblock)
                .map_err(|_| fault("another remote service owns this installation"))?;
            if socket.exists() {
                std::fs::remove_file(&socket)
                    .map_err(|_| fault("cannot replace stale service socket"))?;
            }
            let service = Service::open(&root).await?;
            let listener = UnixListener::bind(&socket)
                .map_err(|_| fault("cannot bind private service socket"))?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
                .map_err(|_| fault("cannot protect service socket"))?;
            let mut term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .map_err(|_| fault("cannot watch shutdown signal"))?;
            let result = tokio::select! {
                result=service.clone().serve(listener)=>result,
                _=term.recv()=>Ok(()),
                _=tokio::signal::ctrl_c()=>Ok(()),
            };
            service.shutdown().await;
            let _ = std::fs::remove_file(&socket);
            result
        }
        Action::Ensure { json } => {
            use remote_codex_protocol::ServiceStart;
            use remote_codex_server::installation::Preparation;
            match remote_codex_server::installation::prepare(&root, &socket).await? {
                Preparation::Ready => {
                    if json {
                        print_status(&ServiceStart::Ready);
                    }
                    return Ok(());
                }
                Preparation::Waiting(activity) => {
                    if json {
                        print_status(&ServiceStart::Waiting(activity));
                        return Ok(());
                    }
                    return Err(activity.fault());
                }
                Preparation::Start => {}
            }
            let log = paths::file(&root.join("service.log"))?;
            let executable =
                std::env::current_exe().map_err(|_| fault("cannot locate service executable"))?;
            let mut child = Command::new(executable)
                .arg("--state")
                .arg(&root)
                .arg("serve")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(log)
                .process_group(0)
                .spawn()
                .map_err(|_| fault("cannot launch remote service"))?;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
            loop {
                if UnixStream::connect(&socket).await.is_ok() {
                    if json {
                        print_status(&ServiceStart::Ready);
                    }
                    return Ok(());
                }
                if child
                    .try_wait()
                    .map_err(|_| fault("cannot inspect service process"))?
                    .is_some()
                {
                    return Err(fault("remote service failed to start"));
                }
                if tokio::time::Instant::now() >= deadline {
                    return Err(fault("remote service startup timed out"));
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        Action::Relay => {
            let stream = UnixStream::connect(&socket)
                .await
                .map_err(|_| fault("remote service is not running"))?;
            let (mut read, mut write) = stream.into_split();
            let mut stdin = tokio::io::stdin();
            let mut stdout = tokio::io::stdout();
            let sender = async {
                use tokio::io::AsyncWriteExt;
                tokio::io::copy(&mut stdin, &mut write).await?;
                write.shutdown().await
            };
            let receiver = tokio::io::copy(&mut read, &mut stdout);
            tokio::pin!(sender, receiver);
            tokio::select! {
                sent = &mut sender => { sent.map_err(|_| fault("relay input closed"))?; receiver.await.map_err(|_| fault("relay output closed"))?; }
                received = &mut receiver => { received.map_err(|_| fault("relay output closed"))?; }
            }
            Ok(())
        }
    }
}

fn fault(message: &str) -> Fault {
    Fault::new("SERVICE_ERROR", message)
}

fn print_status(status: &remote_codex_protocol::ServiceStart) {
    println!("{}", serde_json::json!(status));
}
