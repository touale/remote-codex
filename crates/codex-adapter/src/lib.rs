//! Supervised Codex app-server engine and its bounded stdio protocol transport.
mod defaults;
pub mod engine;
pub mod executor;

pub mod catalog;
pub mod events;
pub mod program;
pub mod recovery;
pub mod thread;

pub mod desktop;
pub mod gateway;
pub mod interactions;

pub mod goals;

pub mod status;

mod tools;
mod usage;
mod usage_history;
