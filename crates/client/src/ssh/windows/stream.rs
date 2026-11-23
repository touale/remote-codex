use crate::ssh::SshProcess;
use russh::{Channel, ChannelMsg, client};
use tokio::{
    io::{AsyncWriteExt, duplex},
    sync::watch,
};

pub(super) fn process(channel: Channel<client::Msg>) -> SshProcess {
    let (mut read, write) = channel.split();
    let input = write.make_writer();
    let (output, mut stdout) = duplex(64 * 1024);
    let (diagnostics, mut stderr) = duplex(64 * 1024);
    let (cancel, mut cancelled) = watch::channel(false);
    let task = tokio::spawn(async move {
        let work = async {
            let mut status = 255;
            while let Some(message) = read.wait().await {
                match message {
                    ChannelMsg::Data { data } => stdout.write_all(&data).await?,
                    ChannelMsg::ExtendedData { data, .. } => stderr.write_all(&data).await?,
                    ChannelMsg::ExitStatus { exit_status } => {
                        status = i32::try_from(exit_status).unwrap_or(255)
                    }
                    ChannelMsg::Close => break,
                    _ => {}
                }
            }
            Ok(status)
        };
        // Cancellation also releases a channel whose output consumer has stalled.
        let result = tokio::select! { result = work => result, _ = cancelled.changed() => Ok(255) };
        let _ = write.close().await;
        result
    });
    SshProcess {
        stdin: Some(Box::pin(input)),
        stdout: Some(Box::pin(output)),
        stderr: Some(Box::pin(diagnostics)),
        cancel,
        task,
    }
}
