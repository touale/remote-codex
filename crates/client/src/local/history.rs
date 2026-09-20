use super::codex_home;
use crate::{ClientError, Result};
use remote_codex_core::session::{HistoryPage, SessionBinding};
use std::path::Path;

pub(crate) async fn read(
    program: &remote_codex_adapter::program::Launch,
    binding: &SessionBinding,
    cursor: Option<&str>,
) -> Result<HistoryPage> {
    if codex_home()? != Path::new(&binding.codex_home) {
        return Err(ClientError::Argument(
            "session belongs to another local CODEX_HOME",
        ));
    }
    Ok(remote_codex_adapter::thread::history::read(program, binding, cursor).await?)
}
