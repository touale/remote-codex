pub mod application;
mod credentials;
mod error;
mod extensions;
mod local;
pub mod progress;
mod remote;
mod runtime;
mod servers;
mod sessions;
mod ssh;
mod store;
mod updates;
mod workspace_lock;

pub use error::{ClientError, Result};
pub use remote_codex_core::{config, connection, session};
pub(crate) use remote_codex_protocol as protocol;

#[cfg(test)]
mod tests;

mod transfers;
