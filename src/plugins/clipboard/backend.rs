//! Platform-neutral text clipboard service.
//!
//! The clipboard plugin depends only on the [`ClipboardService`] trait, not
//! on any particular desktop clipboard API. [`SystemClipboard`] is the
//! desktop clipboard; [`InMemoryClipboard`] is a plain in-process store for
//! tests and headless runs, where there is no display server.
//!
//! Implementations must never log clipboard content; only lengths or
//! booleans derived from it are safe to trace.

use std::sync::{Arc, Mutex};

use thiserror::Error;
use tokio::sync::watch;

mod system;

pub use system::{POLL_INTERVAL, SystemClipboard};

/// Local read/write access to a text clipboard.
pub trait ClipboardService: Send + Sync {
    /// The current clipboard text, if any has been observed.
    fn get(&self) -> Result<Option<String>, ClipboardError>;

    /// Overwrite the clipboard's text content.
    fn set(&self, text: &str) -> Result<(), ClipboardError>;

    /// Text copied on this machine by other applications, for a clipboard
    /// that reports it (see [`SystemClipboard::local_changes`]); `None` by
    /// default.
    fn watch_local_changes(&self) -> Option<watch::Receiver<Option<String>>> {
        None
    }

    /// Let go of the clipboard at shutdown. May block briefly; nothing by
    /// default.
    fn release(&self) {}
}

/// An in-memory clipboard requiring no desktop session, for tests and
/// headless runs.
#[derive(Default)]
pub struct InMemoryClipboard {
    text: Mutex<Option<String>>,
}

impl InMemoryClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience constructor for the common `Arc<dyn ClipboardService>` use.
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

impl ClipboardService for InMemoryClipboard {
    fn get(&self) -> Result<Option<String>, ClipboardError> {
        Ok(self
            .text
            .lock()
            .map_err(|_| ClipboardError::Unavailable)?
            .clone())
    }

    fn set(&self, text: &str) -> Result<(), ClipboardError> {
        *self.text.lock().map_err(|_| ClipboardError::Unavailable)? = Some(text.to_owned());
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("clipboard is unavailable")]
    Unavailable,
    #[error("system clipboard is unavailable: {0}")]
    System(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty_and_round_trips_text() {
        let clipboard = InMemoryClipboard::new();
        assert_eq!(clipboard.get().unwrap(), None);
        clipboard.set("hello").unwrap();
        assert_eq!(clipboard.get().unwrap().as_deref(), Some("hello"));
        clipboard.set("world").unwrap();
        assert_eq!(clipboard.get().unwrap().as_deref(), Some("world"));
    }
}
