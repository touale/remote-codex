use super::*;
use std::{
    cell::RefCell,
    io,
    pin::Pin,
    task::{Context, Poll},
};

struct PartialWriter {
    bytes: Vec<u8>,
    capacity: usize,
    zero_at_capacity: bool,
}

impl AsyncWrite for PartialWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let count = buf.len().min(3).min(self.capacity - self.bytes.len());
        if count == 0 && !self.zero_at_capacity {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        self.bytes.extend_from_slice(&buf[..count]);
        Poll::Ready(Ok(count))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn upload_counts_partial_writes_without_losing_bytes() -> io::Result<()> {
    let source = b"abcdefghij";
    let mut writer = PartialWriter {
        bytes: Vec::new(),
        capacity: 10,
        zero_at_capacity: false,
    };
    let events = RefCell::new(Vec::new());
    let count =
        copy_with_progress(&source[..], &mut writer, |n| events.borrow_mut().push(n)).await?;
    assert_eq!(count, 10);
    assert_eq!(writer.bytes, source);
    assert_eq!(*events.borrow(), [0, 3, 6, 9, 10]);
    Ok(())
}

#[tokio::test]
async fn upload_failures_never_count_unsent_bytes() {
    for zero in [false, true] {
        let mut writer = PartialWriter {
            bytes: Vec::new(),
            capacity: 5,
            zero_at_capacity: zero,
        };
        let events = RefCell::new(Vec::new());
        let result = copy_with_progress(&b"abcdefghij"[..], &mut writer, |n| {
            events.borrow_mut().push(n)
        })
        .await;
        assert_eq!(
            result.err().map(|e| e.kind()),
            Some(if zero {
                io::ErrorKind::WriteZero
            } else {
                io::ErrorKind::BrokenPipe
            })
        );
        assert_eq!(*events.borrow(), [0, 3, 5]);
        assert_eq!(writer.bytes, b"abcde");
    }
}

#[tokio::test]
async fn empty_upload_reports_zero_bytes() -> io::Result<()> {
    let events = RefCell::new(Vec::new());
    let count =
        copy_with_progress(&b""[..], tokio::io::sink(), |n| events.borrow_mut().push(n)).await?;
    assert_eq!(count, 0);
    assert_eq!(*events.borrow(), [0]);
    Ok(())
}
