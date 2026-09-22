//! Shared MyConnect application code.
//!
//! Binary targets should stay thin and call into this library so the CLI and a
//! future GUI can share the same behavior.

pub mod api;
pub mod application;
pub mod client;
pub mod clipboard;
pub mod config;
pub mod device;
pub mod plugins;
pub mod protocol;
pub mod transport;
