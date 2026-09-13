//! Isolated, bounded model fixtures shared by compatibility and service tests.
pub type ProbeResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub mod execution;
pub mod model;
pub mod terminal;
