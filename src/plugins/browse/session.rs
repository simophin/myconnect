//! Browse sessions: one SFTP session per device, opened on first use and
//! shared by every request.
//!
//! Opening one asks the device to serve (`kdeconnect.sftp.request`), waits
//! for its offer, then connects with [`ssh::connect`]. A session is closed
//! when the device disconnects or is unpaired, when the device says its
//! server stopped, when the daemon shuts down, or after [`IDLE_TIMEOUT`]
//! without use. A session stays open while a request or transfer holds it.

use std::{
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll},
    time::{Duration, Instant},
};

use russh_sftp::client::{SftpSession, fs::File};
use tokio::{
    io::{AsyncRead, ReadBuf},
    sync::oneshot,
    time::sleep,
};
use tokio_util::sync::CancellationToken;

use super::{
    BrowseError, REQUEST_PACKET_TYPE,
    packet::{SftpReply, SftpRoot, build_request_packet},
    ssh::{self, SftpConnection, SftpEndpoint, SftpError},
    unix_millis,
};
use crate::core::{CoreError, PluginContext};

/// How long to wait for the device's `kdeconnect.sftp` answer.
const OFFER_TIMEOUT: Duration = Duration::from_secs(5);
/// How long connecting to the device's SFTP server may take. With
/// [`OFFER_TIMEOUT`], this stays inside the API's 15-second request limit.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// How long an unused session stays open.
const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(30);

type SessionSlot = Arc<tokio::sync::Mutex<Option<Arc<RemoteSession>>>>;

/// Every device's session and the offers being waited for.
#[derive(Default)]
pub(super) struct Sessions {
    /// One slot per device. Its async lock serializes opening a session, so
    /// concurrent requests share one instead of each opening their own.
    slots: Mutex<HashMap<String, SessionSlot>>,
    offers: Mutex<HashMap<String, Vec<oneshot::Sender<SftpReply>>>>,
    /// Stops idle-session watchers at shutdown.
    cancellation: CancellationToken,
}

impl Sessions {
    /// The open session with `device_id`, opening one if there is none or
    /// the last one has closed. The device must be paired, connected, and
    /// accept `kdeconnect.sftp.request`.
    pub(super) async fn get(
        &self,
        ctx: &PluginContext,
        device_id: &str,
    ) -> Result<Arc<RemoteSession>, BrowseError> {
        ctx.can_send(device_id, REQUEST_PACKET_TYPE)?;
        let peer = ctx.payload_peer(device_id)?;
        let slot = self.slot(device_id)?;
        let mut current = slot.lock().await;
        if let Some(session) = current.as_ref()
            && !session.connection.is_closed()
        {
            session.touch();
            return Ok(session.clone());
        }
        *current = None;

        let (port, user, password, roots) = match self.request_offer(ctx, device_id).await? {
            SftpReply::Offer {
                port,
                user,
                password,
                roots,
            } => (port, user, password, roots),
            SftpReply::Error(reason) => {
                return Err(BrowseError::Unavailable {
                    reason: Some(reason),
                });
            }
            SftpReply::Stopped => return Err(BrowseError::Failed),
        };
        let endpoint = SftpEndpoint {
            port,
            user,
            password,
        };
        let connection = ssh::connect(&peer, &endpoint, CONNECT_TIMEOUT)
            .await
            .map_err(|error| {
                if matches!(error, SftpError::HostKeyMismatch) {
                    tracing::warn!(device_id, "file server's host key isn't the paired key");
                } else {
                    tracing::debug!(device_id, %error, "opening a browse session failed");
                }
                match error {
                    SftpError::TimedOut => BrowseError::TimedOut,
                    SftpError::HostKeyMismatch | SftpError::UnsupportedPeerKey => {
                        BrowseError::HostKeyMismatch
                    }
                    _ => BrowseError::Failed,
                }
            })?;
        tracing::debug!(device_id, roots = roots.len(), "browse session opened");

        let session = Arc::new(RemoteSession {
            connection,
            roots,
            last_used: Mutex::new(Instant::now()),
        });
        // The device may have disconnected meanwhile, dropping this slot;
        // the session then serves this request only.
        if self.is_current(device_id, &slot) {
            *current = Some(session.clone());
            self.watch_idle(Arc::downgrade(&slot), Arc::downgrade(&session));
        }
        Ok(session)
    }

    fn slot(&self, device_id: &str) -> Result<SessionSlot, BrowseError> {
        let mut slots = self.slots.lock().map_err(|_| CoreError::StateUnavailable)?;
        Ok(slots.entry(device_id.to_owned()).or_default().clone())
    }

    fn is_current(&self, device_id: &str, slot: &SessionSlot) -> bool {
        self.slots
            .lock()
            .ok()
            .and_then(|slots| slots.get(device_id).cloned())
            .is_some_and(|current| Arc::ptr_eq(&current, slot))
    }

    /// Ask the device to serve its files and wait for its answer.
    async fn request_offer(
        &self,
        ctx: &PluginContext,
        device_id: &str,
    ) -> Result<SftpReply, BrowseError> {
        let (sender, receiver) = oneshot::channel();
        self.offers
            .lock()
            .map_err(|_| CoreError::StateUnavailable)?
            .entry(device_id.to_owned())
            .or_default()
            .push(sender);
        let packet = build_request_packet(unix_millis()).map_err(|_| CoreError::Internal)?;
        ctx.send(device_id, packet)?;
        match tokio::time::timeout(OFFER_TIMEOUT, receiver).await {
            Ok(Ok(reply)) => Ok(reply),
            // Waiters are dropped when the device disconnects.
            Ok(Err(_)) => Err(CoreError::DeviceNotConnected.into()),
            Err(_) => Err(BrowseError::TimedOut),
        }
    }

    /// Hand the device's `kdeconnect.sftp` answer to whoever is waiting for
    /// one, or close its session when its server stopped.
    pub(super) fn handle_reply(&self, device_id: &str, reply: SftpReply) {
        if reply == SftpReply::Stopped {
            self.close(device_id);
            return;
        }
        let waiters = self
            .offers
            .lock()
            .ok()
            .and_then(|mut offers| offers.remove(device_id))
            .unwrap_or_default();
        for waiter in waiters {
            let _ = waiter.send(reply.clone());
        }
    }

    /// Forget the device's session and fail anyone waiting for an offer.
    /// The connection closes once requests still using the session finish.
    pub(super) fn close(&self, device_id: &str) {
        if let Ok(mut slots) = self.slots.lock() {
            slots.remove(device_id);
        }
        if let Ok(mut offers) = self.offers.lock() {
            offers.remove(device_id);
        }
    }

    /// Close every session, telling each device. For daemon shutdown, after
    /// transfers have ended, so no transfer still holds a session.
    pub(super) async fn shutdown(&self) {
        self.cancellation.cancel();
        let slots: Vec<SessionSlot> = match self.slots.lock() {
            Ok(mut slots) => slots.drain().map(|(_, slot)| slot).collect(),
            Err(_) => return,
        };
        if let Ok(mut offers) = self.offers.lock() {
            offers.clear();
        }
        for slot in slots {
            let session = slot.lock().await.take();
            if let Some(session) = session.and_then(|session| Arc::try_unwrap(session).ok()) {
                session.connection.close().await;
            }
        }
    }

    /// Close the session once it has gone unused for [`IDLE_TIMEOUT`] with
    /// no request or transfer holding it.
    fn watch_idle(
        &self,
        slot: Weak<tokio::sync::Mutex<Option<Arc<RemoteSession>>>>,
        session: Weak<RemoteSession>,
    ) {
        let cancellation = self.cancellation.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => return,
                    _ = sleep(IDLE_CHECK_INTERVAL) => {}
                }
                let Some(slot) = slot.upgrade() else { return };
                let mut current = slot.lock().await;
                let Some(held) = current.as_ref() else { return };
                if !std::ptr::eq(Arc::as_ptr(held), session.as_ptr()) {
                    return;
                }
                if held.connection.is_closed() {
                    *current = None;
                    return;
                }
                if Arc::strong_count(held) == 1 && held.idle_for() >= IDLE_TIMEOUT {
                    let idle = current.take();
                    drop(current);
                    if let Some(idle) = idle.and_then(|idle| Arc::try_unwrap(idle).ok()) {
                        idle.connection.close().await;
                    }
                    return;
                }
            }
        });
    }
}

/// An open SFTP session with one device, and the roots it offered.
pub(super) struct RemoteSession {
    connection: SftpConnection,
    pub(super) roots: Vec<SftpRoot>,
    last_used: Mutex<Instant>,
}

impl RemoteSession {
    pub(super) fn touch(&self) {
        if let Ok(mut last_used) = self.last_used.lock() {
            *last_used = Instant::now();
        }
    }

    fn idle_for(&self) -> Duration {
        self.last_used
            .lock()
            .map(|last_used| last_used.elapsed())
            .unwrap_or_default()
    }

    pub(super) fn sftp(&self) -> &SftpSession {
        self.connection.session()
    }

    /// Whether the SSH connection has ended, so the session can't be used
    /// again.
    pub(super) fn is_closed(&self) -> bool {
        self.connection.is_closed()
    }
}

/// A file on a device, open for reading. Holds its session open until
/// dropped.
pub struct RemoteFileContent {
    pub name: String,
    pub size: u64,
    pub(super) file: File,
    pub(super) _session: Arc<RemoteSession>,
}

impl AsyncRead for RemoteFileContent {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.file).poll_read(cx, buf)
    }
}
