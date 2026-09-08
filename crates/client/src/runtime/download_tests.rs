use super::*;
use std::{cell::RefCell, io};
use tokio::{net::TcpListener, task::JoinHandle};

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

async fn serve(response: Vec<u8>) -> io::Result<(String, JoinHandle<io::Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/package", listener.local_addr()?);
    let server = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (mut stream, _) = listener.accept().await?;
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await?);
                if request.len() > 16 * 1024 {
                    return Err(io::ErrorKind::InvalidData.into());
                }
            }
            for chunk in response.chunks(32 * 1024) {
                stream.write_all(chunk).await?;
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            stream.shutdown().await
        })
        .await
        .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?
    });
    Ok((url, server))
}

fn client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
}

#[tokio::test]
async fn streams_byte_progress_and_reuses_verified_cache_without_downloading() -> TestResult {
    let root = tempfile::tempdir()?;
    let cache = root.path().join("cache");
    let body = vec![b'x'; 192 * 1024];
    let hash = format!("{:x}", Sha256::digest(&body));
    let mut response =
        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
    response.extend_from_slice(&body);
    let (url, server) = serve(response).await?;
    let events = RefCell::new(Vec::new());
    let client = client()?;
    let path = package_with(&cache, &client, &url, &hash, |e| {
        events.borrow_mut().push(e)
    })
    .await?;
    server.await??;
    assert_eq!(tokio::fs::read(&path).await?, body);
    let transfers: Vec<_> = events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            PrepareEvent::Transfer(t) => Some(*t),
            _ => None,
        })
        .collect();
    assert!(
        transfers.len() > 2,
        "progress must arrive before download completes"
    );
    assert_eq!(transfers[0].transferred_bytes, 0);
    assert_eq!(
        transfers.last().map(|t| t.transferred_bytes),
        Some(body.len() as u64)
    );
    assert!(
        transfers
            .iter()
            .all(|t| t.total_bytes == Some(body.len() as u64))
    );
    assert!(
        transfers
            .windows(2)
            .all(|p| p[0].transferred_bytes < p[1].transferred_bytes)
    );
    assert!(matches!(
        events.borrow().last(),
        Some(PrepareEvent::Stage(PrepareStage::VerifyDownload))
    ));

    events.borrow_mut().clear();
    // The server is now closed; a cache hit must not issue another HTTP request.
    assert_eq!(
        package_with(&cache, &client, &url, &hash, |e| events
            .borrow_mut()
            .push(e))
        .await?,
        path
    );
    assert!(matches!(
        events.borrow().as_slice(),
        [
            PrepareEvent::Stage(PrepareStage::InspectCache),
            PrepareEvent::Stage(PrepareStage::UseCachedPackage),
        ]
    ));
    Ok(())
}

#[tokio::test]
async fn chunked_download_reports_bytes_without_inventing_a_total() -> TestResult {
    let root = tempfile::tempdir()?;
    let cache = root.path().join("cache");
    let (url, server) = serve(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n"
            .to_vec(),
    )
    .await?;
    let events = RefCell::new(Vec::new());
    let hash = format!("{:x}", Sha256::digest(b"abcde"));
    package_with(&cache, &client()?, &url, &hash, |e| {
        events.borrow_mut().push(e)
    })
    .await?;
    server.await??;
    let transfers: Vec<_> = events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            PrepareEvent::Transfer(t) => Some(*t),
            _ => None,
        })
        .collect();
    assert!(transfers.iter().all(|t| t.total_bytes.is_none()));
    assert_eq!(transfers.last().map(|t| t.transferred_bytes), Some(5));
    Ok(())
}

#[tokio::test]
async fn interrupted_download_never_reports_all_bytes_or_publishes_cache() -> TestResult {
    let root = tempfile::tempdir()?;
    let cache = root.path().join("cache");
    let (url, server) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\nabc".to_vec()).await?;
    let events = RefCell::new(Vec::new());
    let result = package_with(&cache, &client()?, &url, &"0".repeat(64), |e| {
        events.borrow_mut().push(e)
    })
    .await;
    server.await??;
    assert!(matches!(result, Err(ClientError::Download(_))));
    assert!(
        !events
            .borrow()
            .iter()
            .any(|e| matches!(e, PrepareEvent::Stage(PrepareStage::VerifyDownload)))
    );
    assert!(events.borrow().iter().all(|e| match e {
        PrepareEvent::Transfer(t) => t.transferred_bytes < 20,
        _ => true,
    }));
    assert_eq!(std::fs::read_dir(&cache)?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn checksum_failure_does_not_publish_cache_even_after_transfer_completes() -> TestResult {
    let root = tempfile::tempdir()?;
    let cache = root.path().join("cache");
    let (url, server) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc".to_vec()).await?;
    let events = RefCell::new(Vec::new());
    let result = package_with(&cache, &client()?, &url, &"0".repeat(64), |e| {
        events.borrow_mut().push(e)
    })
    .await;
    server.await??;
    assert!(matches!(result, Err(ClientError::Integrity)));
    assert!(events.borrow().iter().any(|e| matches!(e,
        PrepareEvent::Transfer(t) if t.transferred_bytes == 3 && t.total_bytes == Some(3)
    )));
    assert_eq!(std::fs::read_dir(&cache)?.count(), 0);
    Ok(())
}
