#[path = "transfers/fixture.rs"]
mod fixture;
use fixture::Fixture;
use remote_codex_client::application::{Client, TransferChoice, TransferProgress, TransferStatus};
use remote_codex_test_support::ProbeResult;
use std::{sync::Arc, time::Duration};

#[tokio::test]
#[ignore = "requires an explicitly authenticated SSH master and packaged service"]
async fn ssh_transfers_recover_without_replaying_or_corrupting_files() -> ProbeResult<()> {
    let (fixture, mut client) = Fixture::open().await?;
    let result = exercise(&fixture, &mut client).await;
    client.close().await;
    let cleanup = fixture.cleanup().await;
    result?;
    cleanup
}
async fn exercise(f: &Fixture, client: &mut Client) -> ProbeResult<()> {
    f.server_files(client).await?;
    let destination_root = f.work().trim_start_matches('/').to_owned();
    let source = f.root.path().join("source");
    std::fs::create_dir_all(source.join("empty"))?;
    let content: Vec<u8> = (0..16 * 1024 * 1024).map(|n| (n % 251) as u8).collect();
    std::fs::write(source.join("binary.dat"), &content)?;
    std::fs::write(source.join(".hidden"), b"hidden")?;
    std::os::unix::fs::symlink("binary.dat", source.join("link"))?;
    let task = client
        .transfers()
        .upload(
            "transfer-test",
            "/",
            &destination_root,
            vec![source.clone()],
        )
        .await?;
    let reached = Arc::new(tokio::sync::Notify::new());
    let notify = reached.clone();
    let progress: TransferProgress = Arc::new(move |t| {
        if t.bytes >= 512 * 1024 {
            notify.notify_one();
        }
    });
    let runner = {
        let c = client.clone();
        let id = task.id.clone();
        tokio::spawn(async move { c.transfers().run(&id, progress).await })
    };
    tokio::time::timeout(Duration::from_secs(30), reached.notified()).await?;
    eprintln!("upload: reached checkpoint, pausing");
    client.transfers().pause(&task.id).await?;
    runner.await??;
    let paused = client.transfers().list().await?.remove(0);
    assert_eq!(paused.status, TransferStatus::Paused);
    assert!(!paused.active);
    assert_eq!(
        f.command(&format!(
            "test ! -e {}/source/binary.dat && printf intact",
            f.work()
        ))
        .await?,
        "intact"
    );
    client.close().await;
    // Both application ownership and remote service are new; durable stage bytes remain.
    eprintln!("upload: restarting isolated service and client");
    f.stop_service().await?;
    *client = Client::open(f.options.clone()).await?;
    assert_eq!(
        client.transfers().list().await?[0].status,
        TransferStatus::Paused
    );
    client.transfers().run(&task.id, Arc::new(|_| {})).await?;
    let completed = client.transfers().list().await?.remove(0);
    assert_eq!(completed.status, TransferStatus::Completed);
    assert_eq!(completed.skipped, 1);
    eprintln!("upload: recovered and verified; starting download outage");
    let destination = f.output()?;
    let download = client
        .transfers()
        .download(
            "transfer-test",
            "/",
            &format!("{destination_root}/source"),
            destination.clone(),
        )
        .await?;
    let reached = Arc::new(tokio::sync::Notify::new());
    let notify = reached.clone();
    let recovered = Arc::new(tokio::sync::Notify::new());
    let recovery = recovered.clone();
    let interrupted = std::sync::atomic::AtomicBool::new(false);
    let reported = std::sync::Mutex::new((TransferStatus::Paused, 0u64));
    let progress = Arc::new(move |t: remote_codex_client::application::Transfer| {
        if let Ok(mut last) = reported.lock() {
            let current = (t.status, t.bytes / (1024 * 1024));
            if *last != current {
                eprintln!("download: {:?}, {}/{} bytes", t.status, t.bytes, t.total);
                *last = current;
            }
        }
        if t.status == TransferStatus::Reconnecting {
            interrupted.store(true, std::sync::atomic::Ordering::Release);
        }
        if t.status == TransferStatus::Running
            && t.bytes >= 1024 * 1024
            && interrupted.load(std::sync::atomic::Ordering::Acquire)
        {
            recovery.notify_one();
        }
        if t.bytes > 0 {
            notify.notify_one();
        }
    });
    let runner = {
        let c = client.clone();
        let id = download.id.clone();
        tokio::spawn(async move { c.transfers().run(&id, progress).await })
    };
    tokio::time::timeout(Duration::from_secs(30), reached.notified()).await?;
    let offline = std::path::PathBuf::from(std::env::var("REMOTE_CODEX_ACCEPTANCE_OUTAGE")?);
    std::fs::write(&offline, b"offline")?;
    fixture::cut_relays()?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    std::fs::remove_file(offline)?;
    // Connection recovery has its own deadline; payload completion also depends on link speed.
    tokio::time::timeout(Duration::from_secs(40), recovered.notified()).await?;
    match tokio::time::timeout(Duration::from_secs(180), runner).await {
        Ok(result) => result??,
        Err(error) => {
            eprintln!("download deadline: {:?}", client.transfers().list().await?);
            return Err(error.into());
        }
    }
    assert_eq!(
        std::fs::read(destination.join("source/binary.dat"))?,
        content
    );
    assert_eq!(
        std::fs::read(destination.join("source/.hidden"))?,
        b"hidden"
    );
    assert!(destination.join("source/empty").is_dir());
    assert!(!destination.join("source/link").exists());
    // A directory collision never silently merges. Keep both remaps every descendant.
    let second = client
        .transfers()
        .upload("transfer-test", &f.work(), "", vec![source])
        .await?;
    client.transfers().run(&second.id, Arc::new(|_| {})).await?;
    assert_eq!(
        client
            .transfers()
            .list()
            .await?
            .iter()
            .find(|t| t.id == second.id)
            .ok_or("task missing")?
            .status,
        TransferStatus::Conflict
    );
    client
        .transfers()
        .resolve(&second.id, TransferChoice::KeepBoth, false)
        .await?;
    client.transfers().run(&second.id, Arc::new(|_| {})).await?;
    assert_eq!(
        f.command(&format!(
            "cmp '{}/source/binary.dat' '{}/source (2)/binary.dat' && printf equal",
            f.work(),
            f.work()
        ))
        .await?,
        "equal"
    );
    // Server terminal needs no WorkspaceHandle and does not register its home as a workspace.
    let shell = client.servers().terminal("transfer-test", 100, 20).await?;
    assert_eq!(
        shell.initial_path(),
        f.command("printf '%s' \"$HOME\"").await?
    );
    shell.close();
    assert!(
        client
            .workspaces()
            .list()
            .await?
            .iter()
            .all(|w| w.path != "/root")
    );
    assert_eq!(
        f.command(&format!(
            "find {} -name '.remote-codex-transfer-*' -print",
            f.work()
        ))
        .await?,
        ""
    );
    client.close().await;
    eprintln!(
        "SSH transfer acceptance: binary upload/download; app/service restart; network recovery; conflicts; empty/hidden entries; skipped links; server terminal; staging cleanup passed"
    );
    Ok(())
}
