//! Supervised Codex app-server engine and its bounded stdio protocol transport.
pub mod engine;
pub mod executor;

pub mod catalog;
pub mod events;
pub mod program;
pub mod recovery;
pub mod thread;

pub mod gateway;
