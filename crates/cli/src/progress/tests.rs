use super::*;

#[test]
fn service_wait_updates_reason_without_resetting_elapsed_time_or_leaving_an_open_line() {
    let now = Instant::now();
    let mut display = PrepareProgress::new(Vec::new(), Mode::Terminal, now);
    display.event(
        PrepareEvent::ServiceWaiting(remote_codex_client::protocol::ServiceActivity {
            channels: 2,
            jobs: 1,
        }),
        now,
    );
    display.event(
        PrepareEvent::ServiceWaiting(remote_codex_client::protocol::ServiceActivity {
            channels: 1,
            jobs: 0,
        }),
        now + Duration::from_secs(8),
    );
    display.finish(now + Duration::from_secs(9));
    let text = String::from_utf8_lossy(&display.writer);
    assert!(text.contains("Waiting for running commands to finish (0s)"));
    assert!(text.contains("Waiting for the remote environment to become idle (9s)"));
    assert!(text.ends_with('\n'));
}
use remote_codex_client::progress::TransferKind;

fn transfer(kind: TransferKind, bytes: u64, total: Option<u64>) -> PrepareEvent {
    PrepareEvent::Transfer(TransferProgress {
        kind,
        transferred_bytes: bytes,
        total_bytes: total,
    })
}

#[test]
fn terminal_shows_transfer_speed_eta_and_finishes_line_before_next_stage() {
    let now = Instant::now();
    let mut display = PrepareProgress::new(Vec::new(), Mode::Terminal, now);
    display.event(PrepareEvent::Stage(PrepareStage::Download), now);
    display.event(transfer(TransferKind::Download, 0, Some(2048)), now);
    display.event(
        transfer(TransferKind::Download, 1024, Some(2048)),
        now + Duration::from_secs(1),
    );
    display.tick(now + Duration::from_secs(1));
    let output = String::from_utf8_lossy(&display.writer);
    assert!(output.contains("50% 1.0KiB/2.0KiB 1.0KiB/s ETA 1s"));
    assert!(output.contains("\r\x1b[2K"));
    display.event(
        transfer(TransferKind::Download, 2048, Some(2048)),
        now + Duration::from_secs(2),
    );
    display.event(
        PrepareEvent::Stage(PrepareStage::VerifyDownload),
        now + Duration::from_secs(2),
    );
    let output = String::from_utf8_lossy(&display.writer);
    assert!(output.contains("100% 2.0KiB/2.0KiB 1.0KiB/s ETA 0s\n"));
    display.finish(now + Duration::from_secs(3));
    assert!(display.writer.ends_with(b"\n"));
    assert!(!display.line_active);
}

#[test]
fn unknown_length_and_stalled_transfers_remain_honest() {
    let now = Instant::now();
    let mut display = PrepareProgress::new(Vec::new(), Mode::Terminal, now);
    display.event(PrepareEvent::Stage(PrepareStage::Download), now);
    display.event(transfer(TransferKind::Download, 1024, None), now);
    display.tick(now + Duration::from_secs(2));
    display.tick(now + Duration::from_secs(4));
    display.finish(now + Duration::from_secs(5));
    let output = String::from_utf8_lossy(&display.writer);
    assert!(output.contains("1.0KiB (total unknown) 512B/s ETA --"));
    assert!(output.contains("1.0KiB (total unknown) 256B/s ETA --"));
    assert!(!output.contains('%'));
    assert!(!output.contains("prepared"));
}

#[test]
fn redirected_progress_is_throttled_plain_text_and_keeps_final_byte_count() {
    let now = Instant::now();
    let mut display = PrepareProgress::new(Vec::new(), Mode::Log, now);
    display.event(PrepareEvent::Stage(PrepareStage::Upload), now);
    display.event(transfer(TransferKind::Upload, 0, Some(100)), now);
    let initial = display.writer.len();
    for n in 1..100 {
        display.event(transfer(TransferKind::Upload, n, Some(100)), now);
        display.tick(now + Duration::from_secs(1));
    }
    assert_eq!(display.writer.len(), initial);
    display.tick(now + Duration::from_secs(5));
    assert!(display.writer.len() > initial);
    display.event(
        transfer(TransferKind::Upload, 100, Some(100)),
        now + Duration::from_secs(6),
    );
    display.event(
        PrepareEvent::Stage(PrepareStage::VerifyInstall),
        now + Duration::from_secs(6),
    );
    let output = String::from_utf8_lossy(&display.writer);
    assert!(output.contains("Upload 100% 100B/100B waiting for SSH"));
    assert!(output.contains("Verifying and installing Codex"));
    assert!(!output.contains(['\r', '\x1b']));
}

#[test]
fn long_checks_update_elapsed_time_without_overwriting_authentication_prompts() {
    let now = Instant::now();
    let mut display = PrepareProgress::new(Vec::new(), Mode::Terminal, now);
    display.event(PrepareEvent::Stage(PrepareStage::ConnectSsh), now);
    let initial = display.writer.len();
    display.tick(now + Duration::from_secs(20));
    assert_eq!(display.writer.len(), initial);
    assert!(display.writer.ends_with(b"\n"));
    display.event(PrepareEvent::Stage(PrepareStage::InspectRuntime), now);
    display.tick(now + Duration::from_secs(10));
    assert!(String::from_utf8_lossy(&display.writer).contains("Checking installed Codex (10s)"));
}

#[test]
fn json_mode_never_emits_progress_control_sequences_or_text() {
    let now = Instant::now();
    let mut display = PrepareProgress::new(Vec::new(), Mode::Quiet, now);
    for stage in [
        PrepareStage::ConnectSsh,
        PrepareStage::Download,
        PrepareStage::Upload,
        PrepareStage::Prepared,
    ] {
        display.event(PrepareEvent::Stage(stage), now);
        display.event(transfer(TransferKind::Download, 1, Some(2)), now);
        display.tick(now + Duration::from_secs(10));
    }
    display.finish(now);
    assert!(display.writer.is_empty());
}

#[test]
fn incomplete_transfer_never_rounds_up_to_one_hundred_percent() {
    let progress = TransferProgress {
        kind: TransferKind::Download,
        transferred_bytes: 9999,
        total_bytes: Some(10000),
    };
    assert!(format::transfer(progress, Duration::from_secs(1)).contains("99%"));
}

#[test]
fn narrow_terminal_frames_do_not_wrap_and_follow_resize() {
    let now = Instant::now();
    let mut display = PrepareProgress::new(Vec::new(), Mode::Terminal, now);
    display.terminal_columns = Some(|| 36);
    display.event(PrepareEvent::Stage(PrepareStage::PrepareSkills), now);
    display.tick(now + Duration::from_secs(1));
    for frame in String::from_utf8_lossy(&display.writer).split("\r\x1b[2K") {
        assert!(console::measure_text_width(frame) < 36);
        assert!(!frame.contains('\n'));
    }
    display.writer.clear();
    display.terminal_columns = Some(|| 100);
    display.tick(now + Duration::from_secs(2));
    assert!(
        String::from_utf8_lossy(&display.writer).contains("Preparing enabled Skill resources (2s)")
    );
}
