use remote_codex_protocol::{MAX_FRAME, codec};
use tokio::io::{AsyncWriteExt, BufReader};

#[tokio::test]
async fn cancelled_read_retains_partial_frame() -> std::io::Result<()> {
    let (mut writer, reader) = tokio::io::duplex(64);
    let mut reader = codec::Reader::new(BufReader::new(reader));
    writer.write_all(b"{\"a\":").await?;
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(5),
            reader.next::<serde_json::Value>()
        )
        .await
        .is_err()
    );
    writer.write_all(b"1}\n").await?;
    assert_eq!(
        reader.next::<serde_json::Value>().await?,
        Some(serde_json::json!({"a":1}))
    );
    Ok(())
}

#[tokio::test]
async fn unterminated_oversized_frame_is_rejected_before_unbounded_allocation()
-> std::io::Result<()> {
    let (mut writer, reader) = tokio::io::duplex(4096);
    let task = tokio::spawn(async move { writer.write_all(&vec![b'x'; MAX_FRAME + 1]).await });
    let mut reader = codec::Reader::new(BufReader::new(reader));
    let error = reader
        .next::<serde_json::Value>()
        .await
        .err()
        .map(|e| e.kind());
    assert_eq!(error, Some(std::io::ErrorKind::InvalidData));
    task.abort();
    Ok(())
}
