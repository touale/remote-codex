//! Shared product rules. This crate does not read files, spawn processes, or
//! depend on a CLI, desktop framework, network runtime, or Codex protocol.

pub mod config;
pub mod connection;

pub mod desktop;
pub mod session;
pub mod status;
pub mod workspace;

pub mod goals;
