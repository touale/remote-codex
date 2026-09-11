use crate::Result;
use std::{future::Future, pin::Pin, sync::Arc};
use zeroize::Zeroizing;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthenticationKind {
    HostKey,
    Password,
    Passphrase,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct AuthenticationPrompt {
    pub target: String,
    pub kind: AuthenticationKind,
    pub message: String,
}
pub type AuthenticationHandler = Arc<
    dyn Fn(
            AuthenticationPrompt,
        ) -> Pin<Box<dyn Future<Output = Result<Option<Zeroizing<String>>>> + Send>>
        + Send
        + Sync,
>;

tokio::task_local! { static CURRENT: Option<AuthenticationHandler>; }
pub(crate) fn current() -> Option<AuthenticationHandler> {
    CURRENT.try_with(Clone::clone).ok().flatten()
}
pub(crate) async fn scope<F: Future>(
    handler: Option<AuthenticationHandler>,
    future: F,
) -> F::Output {
    CURRENT.scope(handler, future).await
}
