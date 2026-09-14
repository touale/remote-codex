//! Standalone peer for exercising replacement without a real Codex process.
use std::{
    env,
    fs::OpenOptions,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::OpenOptionsExt, net::UnixListener},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::args_os().nth(2).ok_or("state missing")?);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(root.join("service.lock"))?;
    lock.lock()?;
    let socket = env::var_os("RC_TEST_SOCKET").ok_or("socket missing")?;
    let response = env::var("RC_TEST_RESPONSE")?;
    let listener = UnixListener::bind(socket)?;
    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut request = String::new();
        if BufReader::new(&mut stream).read_line(&mut request)? == 0 {
            continue;
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("requests.jsonl"))?
            .write_all(request.as_bytes())?;
        if response == "closed" {
            continue;
        }
        if response == "timeout" {
            std::thread::sleep(std::time::Duration::from_secs(4));
            continue;
        }
        stream.write_all(response.as_bytes())?;
        stream.write_all(b"\n")?;
    }
    Ok(())
}
