//! Shared MyConnect application code.
//!
//! Binary targets should stay thin and call into this library so the CLI and a
//! future GUI can share the same behavior.

pub mod api;
pub mod client;
pub mod config;
pub mod core;
pub mod daemon;
pub mod device;
pub mod plugins;
pub mod protocol;
pub mod transport;
