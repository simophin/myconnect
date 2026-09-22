//! Platform-neutral text clipboard service.
//!
//! The application core depends only on the [`ClipboardService`] trait, not
//! on any particular desktop clipboard API. This keeps the MVP testable
//! without a display server: [`InMemoryClipboard`] is a plain in-process
//! store used both by tests and, for now, by the daemon itself. A real OS
//! clipboard backend can be added later behind the same trait without
//! touching `application`.
//!
//! Implementations must never log clipboard content; only lengths or
//! booleans derived from it are safe to trace.

use std::sync::{Arc, Mutex};

use thiserror::Error;

/// Local read/write access to a text clipboard.
pub trait ClipboardService: Send + Sync {
    /// The current clipboard text, if any has been observed.
    fn get(&self) -> Result<Option<String>, ClipboardError>;

    /// Overwrite the clipboard's text content.
    fn set(&self, text: &str) -> Result<(), ClipboardError>;
}

/// An in-memory clipboard requiring no desktop session. Used by tests and as
/// the MVP's only clipboard backend.
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
