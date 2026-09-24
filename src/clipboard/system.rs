//! The desktop clipboard, through the `arboard` crate.
//!
//! `arboard` is synchronous and has no change notifications, so a dedicated
//! thread owns the clipboard: it applies writes as they arrive and polls for
//! text copied by other applications in between. Nothing here blocks the
//! async runtime; [`SystemClipboard::set`] only queues the write.
//!
//! On Linux, `arboard` uses the Wayland data-control protocol when the
//! compositor offers it (KDE Plasma, wlroots compositors) and X11 otherwise,
//! which on GNOME Wayland means XWayland.

use std::{
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tokio::sync::watch;
use tracing::{debug, warn};

use super::{ClipboardError, ClipboardService};

/// How often the clipboard is read to notice text copied locally.
pub const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Text access to a clipboard, as the worker thread needs it: `arboard` in
/// production, a fake in tests.
trait TextBackend {
    /// The clipboard's text, or `None` when it holds none (it is empty, or
    /// holds something else, such as an image).
    fn read(&mut self) -> Result<Option<String>, String>;

    fn write(&mut self, text: &str) -> Result<(), String>;
}

impl TextBackend for arboard::Clipboard {
    fn read(&mut self) -> Result<Option<String>, String> {
        match self.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn write(&mut self, text: &str) -> Result<(), String> {
        self.set_text(text).map_err(|error| error.to_string())
    }
}

enum Request {
    Write(String),
    Stop,
}

/// The desktop clipboard, with local changes reported through
/// [`Self::local_changes`].
///
/// Text already on the clipboard at start is not reported, matching KDE
/// Connect: only what is copied while running is synced. Text written through
/// [`ClipboardService::set`] is not reported back either, so text received
/// from a peer doesn't bounce back to it.
pub struct SystemClipboard {
    requests: mpsc::Sender<Request>,
    /// The text the worker last read or wrote.
    current: Arc<Mutex<Option<String>>>,
    changes: watch::Receiver<Option<String>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl SystemClipboard {
    /// Open the desktop clipboard, failing when there is none (no display
    /// server, or one `arboard` can't talk to).
    pub fn start() -> Result<Self, ClipboardError> {
        Self::spawn(
            || arboard::Clipboard::new().map_err(|error| error.to_string()),
            POLL_INTERVAL,
        )
    }

    fn spawn<B: TextBackend>(
        open: impl FnOnce() -> Result<B, String> + Send + 'static,
        poll_interval: Duration,
    ) -> Result<Self, ClipboardError> {
        let (requests, request_receiver) = mpsc::channel();
        let (opened, opened_receiver) = mpsc::sync_channel(1);
        let current = Arc::new(Mutex::new(None));
        let (changes, change_receiver) = watch::channel(None);
        let worker = {
            let current = current.clone();
            thread::Builder::new()
                .name("myconnect-clipboard".into())
                .spawn(move || {
                    // Opened here because the clipboard may not be `Send`.
                    let backend = match open() {
                        Ok(backend) => {
                            let _ = opened.send(Ok(()));
                            backend
                        }
                        Err(error) => {
                            let _ = opened.send(Err(error));
                            return;
                        }
                    };
                    Worker {
                        backend,
                        current,
                        changes,
                        poll_interval,
                    }
                    .run(request_receiver);
                })
                .map_err(|error| ClipboardError::System(error.to_string()))?
        };
        match opened_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                requests,
                current,
                changes: change_receiver,
                worker: Mutex::new(Some(worker)),
            }),
            Ok(Err(error)) => Err(ClipboardError::System(error)),
            Err(_) => Err(ClipboardError::System(
                "clipboard thread exited during start".into(),
            )),
        }
    }

    /// Text copied locally by other applications. Holds `None` until the
    /// first change; only the latest change is kept.
    pub fn local_changes(&self) -> watch::Receiver<Option<String>> {
        self.changes.clone()
    }

    /// Stop polling and release the clipboard. Idempotent. On X11, text this
    /// process still owns is handed to a clipboard manager if one runs.
    pub fn stop(&self) {
        let _ = self.requests.send(Request::Stop);
        if let Some(worker) = self.worker.lock().ok().and_then(|mut worker| worker.take()) {
            let _ = worker.join();
        }
    }
}

impl ClipboardService for SystemClipboard {
    /// The text last read from or written to the clipboard, which lags a
    /// local copy by up to one poll interval.
    fn get(&self) -> Result<Option<String>, ClipboardError> {
        Ok(self
            .current
            .lock()
            .map_err(|_| ClipboardError::Unavailable)?
            .clone())
    }

    /// Queue `text` to be written; it reaches the clipboard shortly after.
    fn set(&self, text: &str) -> Result<(), ClipboardError> {
        self.requests
            .send(Request::Write(text.to_owned()))
            .map_err(|_| ClipboardError::Unavailable)
    }
}

impl Drop for SystemClipboard {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Worker<B> {
    backend: B,
    current: Arc<Mutex<Option<String>>>,
    changes: watch::Sender<Option<String>>,
    poll_interval: Duration,
}

impl<B: TextBackend> Worker<B> {
    fn run(mut self, requests: mpsc::Receiver<Request>) {
        // Seeded without being reported: see `SystemClipboard`.
        let mut last = self.backend.read().ok().flatten();
        self.remember(&last);
        let mut failing = false;
        let mut next_poll = Instant::now() + self.poll_interval;
        loop {
            match requests.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
                Ok(Request::Write(text)) => match self.backend.write(&text) {
                    Ok(()) => {
                        last = Some(text);
                        self.remember(&last);
                    }
                    Err(error) => {
                        warn!(%error, length = text.len(), "could not write the system clipboard");
                    }
                },
                Ok(Request::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    next_poll = Instant::now() + self.poll_interval;
                    match self.backend.read() {
                        Ok(Some(text)) if last.as_ref() != Some(&text) => {
                            failing = false;
                            debug!(length = text.len(), "system clipboard changed");
                            last = Some(text.clone());
                            self.remember(&last);
                            if !text.is_empty() {
                                self.changes.send_replace(Some(text));
                            }
                        }
                        // Unchanged, or no text (e.g. an image was copied).
                        Ok(_) => failing = false,
                        Err(error) => {
                            // Logged once per run of failures, not every poll.
                            if !failing {
                                warn!(%error, "could not read the system clipboard");
                            }
                            failing = true;
                        }
                    }
                }
            }
        }
    }

    fn remember(&self, text: &Option<String>) {
        if let Ok(mut current) = self.current.lock() {
            current.clone_from(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clipboard other "applications" change through the shared cell.
    #[derive(Clone, Default)]
    struct FakeBackend(Arc<Mutex<Option<String>>>);

    impl FakeBackend {
        fn copy(&self, text: &str) {
            *self.0.lock().unwrap() = Some(text.to_owned());
        }

        fn text(&self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }
    }

    impl TextBackend for FakeBackend {
        fn read(&mut self) -> Result<Option<String>, String> {
            Ok(self.text())
        }

        fn write(&mut self, text: &str) -> Result<(), String> {
            self.copy(text);
            Ok(())
        }
    }

    const POLL: Duration = Duration::from_millis(10);

    fn start(backend: &FakeBackend) -> SystemClipboard {
        let backend = backend.clone();
        SystemClipboard::spawn(move || Ok(backend), POLL).unwrap()
    }

    async fn next_change(changes: &mut watch::Receiver<Option<String>>) -> Option<String> {
        tokio::time::timeout(Duration::from_secs(5), changes.changed())
            .await
            .expect("a change is reported")
            .unwrap();
        changes.borrow_and_update().clone()
    }

    /// Long enough for several polls.
    async fn settle() {
        tokio::time::sleep(POLL * 10).await;
    }

    #[tokio::test]
    async fn reports_local_copies_but_not_what_was_there_at_start() {
        let backend = FakeBackend::default();
        backend.copy("before start");
        let clipboard = start(&backend);
        let mut changes = clipboard.local_changes();
        settle().await;
        assert!(!changes.has_changed().unwrap());
        assert_eq!(clipboard.get().unwrap().as_deref(), Some("before start"));

        backend.copy("copied");
        assert_eq!(next_change(&mut changes).await.as_deref(), Some("copied"));
        assert_eq!(clipboard.get().unwrap().as_deref(), Some("copied"));
    }

    #[tokio::test]
    async fn written_text_is_not_reported_back() {
        let backend = FakeBackend::default();
        let clipboard = start(&backend);
        let mut changes = clipboard.local_changes();

        clipboard.set("from a peer").unwrap();
        settle().await;
        assert_eq!(backend.text().as_deref(), Some("from a peer"));
        assert!(!changes.has_changed().unwrap());

        backend.copy("local");
        assert_eq!(next_change(&mut changes).await.as_deref(), Some("local"));
    }

    #[tokio::test]
    async fn clearing_the_clipboard_is_not_reported() {
        let backend = FakeBackend::default();
        let clipboard = start(&backend);
        let mut changes = clipboard.local_changes();
        backend.copy("text");
        assert_eq!(next_change(&mut changes).await.as_deref(), Some("text"));

        *backend.0.lock().unwrap() = None;
        settle().await;
        backend.copy("");
        settle().await;
        assert!(!changes.has_changed().unwrap());
    }

    #[test]
    fn a_clipboard_that_cannot_be_opened_fails_to_start() {
        let result = SystemClipboard::spawn::<FakeBackend>(|| Err("no display".into()), POLL);
        assert!(matches!(result, Err(ClipboardError::System(error)) if error == "no display"));
    }

    #[test]
    fn stop_ends_the_worker_and_later_writes_fail() {
        let clipboard = start(&FakeBackend::default());
        clipboard.stop();
        clipboard.stop();
        assert!(matches!(
            clipboard.set("late"),
            Err(ClipboardError::Unavailable)
        ));
    }
}
