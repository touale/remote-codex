use crate::{Checked, Result, service::Service};
use nix::fcntl::{Flock, FlockArg};
use remote_codex_protocol::{
    Call, Fault, Frame, Request, codec,
    transfer::{Command, Reply, wire},
};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

pub(crate) async fn serve(
    service: &Arc<Service>,
    call: &Call,
    mut read: impl AsyncRead + Unpin,
    mut write: impl AsyncWrite + Unpin,
) -> Result<()> {
    let open = || {
        service.validate(call)?;
        if call.expected_identity.as_deref() != Some(&service.store.identity) {
            return Err(Fault::new(
                "TRANSFER_IDENTITY",
                "File transfer requires a verified server identity.",
            ));
        }
        let Request::Transfer {
            workspace,
            expected_root,
        } = &call.request
        else {
            return Err(Fault::new(
                "TRANSFER_REQUEST",
                "Invalid transfer handshake.",
            ));
        };
        let file = crate::paths::file(&service.root.join("transfer.lock"))?;
        let lock = Flock::lock(file, FlockArg::LockSharedNonblock)
            .map_err(|_| Fault::new("SERVICE_UPDATE_BUSY", "Service update is in progress."))?;
        let endpoint = remote_codex_transfer::Endpoint::open(Path::new(workspace), &call.profile)?;
        let identity = endpoint.identity()?;
        if expected_root
            .as_ref()
            .is_some_and(|expected| expected != &identity)
        {
            return Err(Fault::new(
                "TRANSFER_ROOT_CHANGED",
                "Workspace directory changed. Select the workspace again.",
            ));
        }
        Ok(Arc::new((endpoint, lock)))
    };
    let endpoint = match open() {
        Ok(endpoint) => endpoint,
        Err(error) => {
            codec::write(&mut write, &Frame::Error(error.clone()))
                .await
                .checked("TRANSFER_CONNECTION", "Transfer disconnected")?;
            return Err(error);
        }
    };
    codec::write(
        &mut write,
        &Frame::Result(
            serde_json::to_value(endpoint.0.identity()?)
                .checked("TRANSFER_IDENTITY", "Cannot encode directory identity.")?,
        ),
    )
    .await
    .checked("TRANSFER_CONNECTION", "Transfer disconnected")?;
    loop {
        let (command, bytes) =
            tokio::time::timeout(Duration::from_secs(600), wire::read::<Command>(&mut read))
                .await
                .checked("TRANSFER_TIMEOUT", "Transfer connection timed out.")?
                .checked("TRANSFER_CONNECTION", "Transfer connection closed.")?;
        let worker_endpoint = endpoint.clone();
        let mut worker =
            tokio::task::spawn_blocking(move || worker_endpoint.0.dispatch(command, &bytes));
        let mut probe = [0; 1];
        let result = tokio::select! {
            result = &mut worker => result.checked("TRANSFER_SERVICE", "Transfer worker stopped.")?,
            _ = read.read(&mut probe) => {
                // This stream allows one request at a time. EOF also cancels long checksums.
                endpoint.0.stop();
                let _ = worker.await;
                return Err(Fault::new("TRANSFER_CONNECTION", "Transfer peer disconnected or sent an overlapping request."));
            }
        };
        let (reply, bytes): (std::result::Result<Reply, Fault>, Vec<u8>) = match result {
            Ok((reply, bytes)) => (Ok(reply), bytes),
            Err(error) => (Err(error), Vec::new()),
        };
        tokio::time::timeout(
            Duration::from_secs(60),
            wire::write(&mut write, &reply, &bytes),
        )
        .await
        .checked("TRANSFER_TIMEOUT", "Transfer peer stopped reading.")?
        .checked("TRANSFER_CONNECTION", "Transfer connection closed.")?;
    }
}
