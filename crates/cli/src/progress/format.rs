use remote_codex_client::progress::{PrepareStage, TransferKind, TransferProgress};
use std::time::Duration;

pub(super) fn stage(stage: PrepareStage) -> &'static str {
    match stage {
        PrepareStage::StartLocalCodex => "Starting local Codex",
        PrepareStage::ConnectExecution => "Connecting execution environment",
        PrepareStage::PrepareSkills => "Preparing enabled Skill resources",
        PrepareStage::OpenLocalSession => "Opening local session and MCP servers",
        PrepareStage::ConnectSsh => "Connecting to SSH (authentication may be required)",
        PrepareStage::InspectHost => "Checking remote system",
        PrepareStage::InspectRuntime => "Checking installed Codex",
        PrepareStage::InspectCache => "Checking cached Codex package",
        PrepareStage::UseCachedPackage => "Using verified cached Codex package",
        PrepareStage::Download => "Downloading Codex package (waiting for response)",
        PrepareStage::VerifyDownload => "Verifying downloaded Codex package",
        PrepareStage::Upload => "Uploading Codex package",
        PrepareStage::VerifyInstall => "Verifying and installing Codex",
        PrepareStage::Prepared => "Codex prepared",
        PrepareStage::InstallService => "Installing remote execution service",
        PrepareStage::StartService => "Starting remote execution service",
        PrepareStage::UseRunningService => {
            "Using the compatible running service; update deferred until idle"
        }
        PrepareStage::Synchronize => "Applying server configuration",
    }
}

pub(super) fn transfer(progress: TransferProgress, elapsed: Duration) -> String {
    let label = match progress.kind {
        TransferKind::Download => "Download",
        TransferKind::Upload => "Upload",
    };
    let done = progress.transferred_bytes;
    let total = progress.total_bytes;
    let amount = match total {
        Some(total) => {
            let percent = if total == 0 {
                100.0
            } else {
                (done as f64 / total as f64 * 100.0).min(100.0)
            };
            // Rounding must not show 100% while there are still bytes to transfer.
            format!("{:3.0}% {}/{}", percent.floor(), bytes(done), bytes(total))
        }
        None => format!("{} (total unknown)", bytes(done)),
    };
    if progress.kind == TransferKind::Upload && total.is_some_and(|total| done >= total) {
        return format!("{label} {amount} waiting for SSH ({}s)", elapsed.as_secs());
    }
    let rate = if elapsed.is_zero() {
        0.0
    } else {
        done as f64 / elapsed.as_secs_f64()
    };
    let speed = if rate > 0.0 {
        format!("{}/s", bytes(rate as u64))
    } else {
        "--/s".to_owned()
    };
    let eta = match total {
        Some(total) if rate > 0.0 => {
            duration((total.saturating_sub(done) as f64 / rate).ceil() as u64)
        }
        _ => "--".to_owned(),
    };
    format!("{label} {amount} {speed} ETA {eta}")
}

fn bytes(value: u64) -> String {
    let value = value as f64;
    for (unit, scale) in [("GiB", 1073741824.0), ("MiB", 1048576.0), ("KiB", 1024.0)] {
        if value >= scale {
            return format!("{:.1}{unit}", value / scale);
        }
    }
    format!("{value:.0}B")
}

fn duration(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}h{}m", seconds / 3600, seconds % 3600 / 60)
    } else if seconds >= 60 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}
