//! Browsing a paired peer's files over its SFTP server (§12 of
//! ARCHITECTURE.md).
//!
//! A session is opened on first use: ask the peer to serve
//! (`kdeconnect.sftp.request`), wait for its offer, then connect with
//! [`sftp::connect`]. One session per device is kept and shared by every
//! request. It is closed when the device disconnects or is forgotten, when
//! the peer says its server stopped, when the daemon shuts down, or after
//! [`IDLE_TIMEOUT`] without use. Downloads and uploads run as ordinary
//! transfer resources, so they show up, progress and cancel like any other.

use std::{
    pin::Pin,
    sync::Weak,
    task::{Context, Poll},
};

use russh_sftp::{
    client::{error::Error as SftpStatusError, fs::Metadata},
    protocol::{FileType, OpenFlags, StatusCode},
};
use tokio::{
    io::{AsyncRead, AsyncWriteExt, ReadBuf},
    sync::oneshot,
};

use super::*;
use crate::{
    application::files::{
        DirectoryListing, FileEntry, FileKind, join_remote_path, normalize_remote_path,
        numbered_name, split_remote_path, validate_remote_name,
    },
    plugins::sftp::{self, SftpBody, SftpReply, SftpRoot},
    transport::sftp::{self as sftp_transport, SftpConnection, SftpEndpoint, SftpError},
};

/// How long to wait for the peer's `kdeconnect.sftp` answer.
const OFFER_TIMEOUT: Duration = Duration::from_secs(5);
/// How long connecting to the peer's SFTP server may take. With
/// [`OFFER_TIMEOUT`], this stays inside the API's 15-second request limit.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// How long an unused session stays open.
const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(30);

type SessionSlot = Arc<tokio::sync::Mutex<Option<Arc<RemoteSession>>>>;

/// Browse sessions and the offers being waited for, shared by every clone
/// of the handle.
#[derive(Default)]
pub(super) struct Browsing {
    /// One slot per device. Its async lock serializes opening a session, so
    /// concurrent requests share one instead of each opening their own.
    sessions: Mutex<HashMap<String, SessionSlot>>,
    offers: Mutex<HashMap<String, Vec<oneshot::Sender<SftpReply>>>>,
    /// Stops idle-session watchers at shutdown.
    cancellation: CancellationToken,
}

impl Browsing {
    fn slot(&self, device_id: &str) -> Option<SessionSlot> {
        let mut sessions = self.sessions.lock().ok()?;
        Some(sessions.entry(device_id.to_owned()).or_default().clone())
    }

    fn is_current(&self, device_id: &str, slot: &SessionSlot) -> bool {
        self.sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(device_id).cloned())
            .is_some_and(|current| Arc::ptr_eq(&current, slot))
    }
}

/// An open SFTP session with one peer, and the roots it offered.
struct RemoteSession {
    connection: SftpConnection,
    roots: Vec<SftpRoot>,
    last_used: Mutex<Instant>,
}

impl RemoteSession {
    fn touch(&self) {
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

    fn sftp(&self) -> &russh_sftp::client::SftpSession {
        self.connection.session()
    }
}

/// A file on a peer, open for reading. Holds its session open until
/// dropped.
pub struct RemoteFileContent {
    pub name: String,
    pub size: u64,
    file: russh_sftp::client::fs::File,
    _session: Arc<RemoteSession>,
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

impl ApplicationHandle {
    /// List a directory on a peer, or, with no `path`, the storage roots it
    /// exposes.
    pub async fn list_files(
        &self,
        device_id: &str,
        path: Option<String>,
    ) -> Result<DirectoryListing, ApplicationError> {
        let session = self.remote_session(device_id).await?;
        let Some(path) = path else {
            return Ok(DirectoryListing {
                path: None,
                entries: session
                    .roots
                    .iter()
                    .filter_map(|root| {
                        Some(FileEntry {
                            name: root.name.clone(),
                            path: normalize_remote_path(&root.path).ok()?,
                            kind: FileKind::Directory,
                            size: None,
                            modified_at: None,
                        })
                    })
                    .collect(),
            });
        };
        let path = normalize_path(&path)?;
        let entries = match session.sftp().read_dir(path.as_str()).await {
            Ok(entries) => entries,
            Err(error) => {
                // Reading a file as a directory fails with a generic status;
                // tell that case apart.
                if let Ok(metadata) = session.sftp().metadata(path.as_str()).await
                    && !metadata.is_dir()
                {
                    return Err(ApplicationError::NotADirectory);
                }
                return Err(self.remote_error(device_id, &session, error));
            }
        };
        let mut listed = Vec::new();
        for entry in entries {
            let name = entry.file_name();
            let entry_path = join_remote_path(&path, &name);
            let mut metadata = entry.metadata();
            // Show a link as what it points to, so a linked directory can be
            // opened; a dangling link stays a link.
            if metadata.file_type() == FileType::Symlink
                && let Ok(target) = session.sftp().metadata(entry_path.as_str()).await
            {
                metadata = target;
            }
            listed.push(file_entry(entry_path, name, &metadata));
        }
        listed.sort_by(|a, b| a.name.cmp(&b.name));
        let entries = listed;
        Ok(DirectoryListing {
            path: Some(path),
            entries,
        })
    }

    /// Open a file on a peer for reading, e.g. to show a preview.
    pub async fn open_remote_file(
        &self,
        device_id: &str,
        path: &str,
    ) -> Result<RemoteFileContent, ApplicationError> {
        let path = normalize_path(path)?;
        let (_, name) = split_remote_path(&path).ok_or(ApplicationError::IsADirectory)?;
        let session = self.remote_session(device_id).await?;
        let size = self.remote_file_size(device_id, &session, &path).await?;
        let file = session
            .sftp()
            .open(path.as_str())
            .await
            .map_err(|error| self.remote_error(device_id, &session, error))?;
        Ok(RemoteFileContent {
            name: name.to_owned(),
            size,
            file,
            _session: session,
        })
    }

    /// Start downloading a file from a peer into the download directory, as
    /// an incoming transfer.
    pub async fn begin_file_download(
        &self,
        device_id: &str,
        path: &str,
    ) -> Result<TransferSnapshot, ApplicationError> {
        let path = normalize_path(path)?;
        let (_, name) = split_remote_path(&path).ok_or(ApplicationError::IsADirectory)?;
        let file_name = sanitize_file_name(name).map_err(|_| ApplicationError::InvalidFileName)?;
        let session = self.remote_session(device_id).await?;
        let total = self.remote_file_size(device_id, &session, &path).await?;
        if total > self.transfer_config.max_transfer_bytes {
            return Err(ApplicationError::TransferTooLarge {
                limit: self.transfer_config.max_transfer_bytes,
            });
        }

        let transfer_id = Uuid::new_v4();
        let transfer = self.new_transfer(
            transfer_id,
            device_id,
            TransferDirection::Incoming,
            file_name.clone(),
            total,
        )?;
        let started = transfer.snapshot();
        self.insert_transfer(transfer)?;
        let cancellation = CancellationToken::new();
        let task_handle = self.clone();
        let task_cancellation = cancellation.clone();
        let join = tokio::spawn(async move {
            task_handle
                .run_file_download(
                    transfer_id,
                    session,
                    path,
                    file_name,
                    total,
                    task_cancellation,
                )
                .await;
        });
        self.track_transfer_task(transfer_id, device_id, cancellation, join);
        self.events
            .publish(super::super::EventData::TransferStarted(started.clone()))?;
        Ok(started)
    }

    async fn run_file_download(
        &self,
        transfer_id: Uuid,
        session: Arc<RemoteSession>,
        path: String,
        file_name: String,
        total: u64,
        cancellation: CancellationToken,
    ) {
        let mut remote = tokio::select! {
            _ = cancellation.cancelled() => {
                self.finish_transfer_cancelled(transfer_id);
                self.cleanup_transfer_task(transfer_id);
                return;
            }
            result = session.sftp().open(path.as_str()) => match result {
                Ok(file) => file,
                Err(_) => {
                    self.fail_transfer(transfer_id, OperationErrorCode::ConnectionFailed);
                    self.cleanup_transfer_task(transfer_id);
                    return;
                }
            },
        };

        // Read once, so the partial and the final file share a directory
        // even if the setting changes mid-transfer.
        let download_dir = match self.settings() {
            Ok(settings) => settings.download_dir,
            Err(_) => {
                self.fail_transfer(transfer_id, OperationErrorCode::Internal);
                self.cleanup_transfer_task(transfer_id);
                return;
            }
        };
        if tokio::fs::create_dir_all(&download_dir).await.is_err() {
            self.fail_transfer(transfer_id, OperationErrorCode::Internal);
            self.cleanup_transfer_task(transfer_id);
            return;
        }
        let temp_path = download_dir.join(format!(".{transfer_id}.part"));
        let mut file = match tokio::fs::File::create(&temp_path).await {
            Ok(file) => file,
            Err(_) => {
                self.fail_transfer(transfer_id, OperationErrorCode::Internal);
                self.cleanup_transfer_task(transfer_id);
                return;
            }
        };
        if self
            .transition_transfer(transfer_id, TransferStatus::Transferring, None)
            .is_none()
        {
            let _ = tokio::fs::remove_file(&temp_path).await;
            self.cleanup_transfer_task(transfer_id);
            return;
        }

        let mut progress = ProgressEvents::default();
        let result = payload::copy_exact(
            &mut remote,
            &mut file,
            total,
            &cancellation,
            |transferred| {
                session.touch();
                progress.record(self, transfer_id, transferred);
            },
        )
        .await;
        drop(file);
        let _ = remote.shutdown().await;

        match result {
            Ok(()) => {
                let destination = unique_destination(&download_dir, &file_name);
                match tokio::fs::rename(&temp_path, &destination).await {
                    Ok(()) => self.complete_transfer(
                        transfer_id,
                        Some(std::path::absolute(&destination).unwrap_or(destination)),
                    ),
                    Err(_) => {
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        self.fail_transfer(transfer_id, OperationErrorCode::Internal);
                    }
                }
            }
            Err(payload::PayloadError::Cancelled) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                self.finish_transfer_cancelled(transfer_id);
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(&temp_path).await;
                self.fail_transfer(transfer_id, OperationErrorCode::ConnectionFailed);
            }
        }
        self.cleanup_transfer_task(transfer_id);
    }

    /// Start uploading a file into `directory` on a peer, as an outgoing
    /// transfer. The file is created before this returns, under `file_name`
    /// or, if that is taken, a numbered variant of it, so an upload never
    /// replaces an existing file. The caller streams the content into the
    /// returned sender; an upload that ends early is removed from the peer.
    pub async fn begin_file_upload(
        &self,
        device_id: &str,
        directory: &str,
        file_name: &str,
        declared_size: u64,
    ) -> Result<(TransferSnapshot, mpsc::Sender<Bytes>), ApplicationError> {
        let directory = normalize_path(directory)?;
        validate_remote_name(file_name).map_err(|_| ApplicationError::InvalidFileName)?;
        if declared_size > self.transfer_config.max_transfer_bytes {
            return Err(ApplicationError::TransferTooLarge {
                limit: self.transfer_config.max_transfer_bytes,
            });
        }
        let session = self.remote_session(device_id).await?;
        let metadata = session
            .sftp()
            .metadata(directory.as_str())
            .await
            .map_err(|error| self.remote_error(device_id, &session, error))?;
        if !metadata.is_dir() {
            return Err(ApplicationError::NotADirectory);
        }

        let (path, mut remote) = self
            .create_unique_file(device_id, &session, &directory, file_name)
            .await?;
        let (_, final_name) = split_remote_path(&path).expect("created under a directory");
        let transfer_id = Uuid::new_v4();
        let transfer = self.new_transfer(
            transfer_id,
            device_id,
            TransferDirection::Outgoing,
            final_name.to_owned(),
            declared_size,
        )?;
        let started = transfer.snapshot();
        if let Err(error) = self.insert_transfer(transfer) {
            let _ = remote.shutdown().await;
            let _ = session.sftp().remove_file(path.as_str()).await;
            return Err(error);
        }

        let (chunk_tx, chunk_rx) = mpsc::channel::<Bytes>(TRANSFER_CHANNEL_CAPACITY);
        let cancellation = CancellationToken::new();
        let task_handle = self.clone();
        let task_cancellation = cancellation.clone();
        let join = tokio::spawn(async move {
            task_handle
                .run_file_upload(
                    transfer_id,
                    session,
                    path,
                    remote,
                    chunk_rx,
                    task_cancellation,
                )
                .await;
        });
        self.track_transfer_task(transfer_id, device_id, cancellation, join);
        self.events
            .publish(super::super::EventData::TransferStarted(started.clone()))?;
        Ok((started, chunk_tx))
    }

    /// Create `name` in `directory` without replacing anything: on a
    /// collision, try `name (1)`, `name (2)`, and so on.
    async fn create_unique_file(
        &self,
        device_id: &str,
        session: &Arc<RemoteSession>,
        directory: &str,
        name: &str,
    ) -> Result<(String, russh_sftp::client::fs::File), ApplicationError> {
        const MAX_ATTEMPTS: u32 = 1000;
        for attempt in 0..MAX_ATTEMPTS {
            let candidate = if attempt == 0 {
                name.to_owned()
            } else {
                numbered_name(name, attempt)
            };
            let path = join_remote_path(directory, &candidate);
            // SFTP v3 servers report "already exists" as a generic failure,
            // so check first; `EXCLUDE` still guards against a race.
            if self.remote_exists(device_id, session, &path).await? {
                continue;
            }
            return match session
                .sftp()
                .open_with_flags(
                    path.as_str(),
                    OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE,
                )
                .await
            {
                Ok(file) => Ok((path, file)),
                Err(error) => Err(self.remote_error(device_id, session, error)),
            };
        }
        Err(ApplicationError::RemoteFileExists)
    }

    async fn run_file_upload(
        &self,
        transfer_id: Uuid,
        session: Arc<RemoteSession>,
        path: String,
        mut remote: russh_sftp::client::fs::File,
        mut chunk_rx: mpsc::Receiver<Bytes>,
        cancellation: CancellationToken,
    ) {
        let Some(total) = self.transfer_snapshot(transfer_id).map(|s| s.total_bytes) else {
            self.cleanup_transfer_task(transfer_id);
            return;
        };
        if self
            .transition_transfer(transfer_id, TransferStatus::Transferring, None)
            .is_none()
        {
            let _ = session.sftp().remove_file(path.as_str()).await;
            self.cleanup_transfer_task(transfer_id);
            return;
        }

        let mut progress = ProgressEvents::default();
        let mut result = payload::forward_channel(
            &mut chunk_rx,
            &mut remote,
            total,
            &cancellation,
            |transferred| {
                session.touch();
                progress.record(self, transfer_id, transferred);
            },
        )
        .await;
        // Closing the handle is when a server reports a failed write.
        let closed = remote.shutdown().await;
        if result.is_ok()
            && let Err(error) = closed
        {
            result = Err(payload::PayloadError::Socket(error));
        }

        match result {
            Ok(()) => self.complete_transfer(transfer_id, None),
            Err(error) => {
                let _ = session.sftp().remove_file(path.as_str()).await;
                if matches!(error, payload::PayloadError::Cancelled) {
                    self.finish_transfer_cancelled(transfer_id);
                } else {
                    self.fail_transfer(transfer_id, OperationErrorCode::ConnectionFailed);
                }
            }
        }
        self.cleanup_transfer_task(transfer_id);
    }

    /// Create a directory on a peer.
    pub async fn create_remote_directory(
        &self,
        device_id: &str,
        path: &str,
    ) -> Result<FileEntry, ApplicationError> {
        let path = normalize_path(path)?;
        split_remote_path(&path).ok_or(ApplicationError::RemoteFileExists)?;
        let session = self.remote_session(device_id).await?;
        if self.remote_exists(device_id, &session, &path).await? {
            return Err(ApplicationError::RemoteFileExists);
        }
        session
            .sftp()
            .create_dir(path.as_str())
            .await
            .map_err(|error| self.remote_error(device_id, &session, error))?;
        self.remote_entry(device_id, &session, path).await
    }

    /// Move or rename a file or directory on a peer. Never replaces an
    /// existing file.
    pub async fn move_remote_file(
        &self,
        device_id: &str,
        from: &str,
        to: &str,
    ) -> Result<FileEntry, ApplicationError> {
        let from = normalize_path(from)?;
        let to = normalize_path(to)?;
        let session = self.remote_session(device_id).await?;
        if self.is_protected(&session, &from) || split_remote_path(&to).is_none() {
            return Err(ApplicationError::InvalidRemotePath);
        }
        if self.remote_exists(device_id, &session, &to).await? {
            return Err(ApplicationError::RemoteFileExists);
        }
        session
            .sftp()
            .rename(from.as_str(), to.as_str())
            .await
            .map_err(|error| self.remote_error(device_id, &session, error))?;
        self.remote_entry(device_id, &session, to).await
    }

    /// Delete a file, or a directory with everything in it, on a peer.
    /// Symbolic links are removed, never followed. The storage roots
    /// themselves can't be deleted.
    pub async fn delete_remote_file(
        &self,
        device_id: &str,
        path: &str,
    ) -> Result<(), ApplicationError> {
        let path = normalize_path(path)?;
        let session = self.remote_session(device_id).await?;
        if self.is_protected(&session, &path) {
            return Err(ApplicationError::InvalidRemotePath);
        }
        let sftp = session.sftp();
        let remote_error = |error| self.remote_error(device_id, &session, error);

        // Depth first: a directory is removed once everything under it has
        // been.
        let mut pending = vec![(path, false)];
        while let Some((path, visited)) = pending.pop() {
            let metadata = sftp
                .symlink_metadata(path.as_str())
                .await
                .map_err(remote_error)?;
            if !metadata.is_dir() {
                sftp.remove_file(path.as_str())
                    .await
                    .map_err(remote_error)?;
            } else if visited {
                sftp.remove_dir(path.as_str()).await.map_err(remote_error)?;
            } else {
                let children = sftp.read_dir(path.as_str()).await.map_err(remote_error)?;
                pending.push((path.clone(), true));
                pending.extend(
                    children.map(|entry| (join_remote_path(&path, &entry.file_name()), false)),
                );
            }
            session.touch();
        }
        Ok(())
    }

    /// Whether `path` is `/` or one of the peer's storage roots, which the
    /// user may browse but not move or delete.
    fn is_protected(&self, session: &RemoteSession, path: &str) -> bool {
        path == "/"
            || session
                .roots
                .iter()
                .filter_map(|root| normalize_remote_path(&root.path).ok())
                .any(|root| root == path)
    }

    async fn remote_exists(
        &self,
        device_id: &str,
        session: &Arc<RemoteSession>,
        path: &str,
    ) -> Result<bool, ApplicationError> {
        match session.sftp().symlink_metadata(path).await {
            Ok(_) => Ok(true),
            Err(SftpStatusError::Status(status))
                if status.status_code == StatusCode::NoSuchFile =>
            {
                Ok(false)
            }
            Err(error) => Err(self.remote_error(device_id, session, error)),
        }
    }

    async fn remote_entry(
        &self,
        device_id: &str,
        session: &Arc<RemoteSession>,
        path: String,
    ) -> Result<FileEntry, ApplicationError> {
        let metadata = session
            .sftp()
            .symlink_metadata(path.as_str())
            .await
            .map_err(|error| self.remote_error(device_id, session, error))?;
        let name = split_remote_path(&path)
            .map(|(_, name)| name.to_owned())
            .unwrap_or_else(|| "/".to_owned());
        Ok(file_entry(path, name, &metadata))
    }

    /// The size of the regular file at `path`, following links.
    async fn remote_file_size(
        &self,
        device_id: &str,
        session: &Arc<RemoteSession>,
        path: &str,
    ) -> Result<u64, ApplicationError> {
        let metadata = session
            .sftp()
            .metadata(path)
            .await
            .map_err(|error| self.remote_error(device_id, session, error))?;
        if metadata.is_dir() {
            return Err(ApplicationError::IsADirectory);
        }
        metadata.size.ok_or(ApplicationError::RemoteFilesFailed)
    }

    /// Map an SFTP failure to an API error. A failure that ended the
    /// connection also drops the session, so the next request opens a new
    /// one.
    fn remote_error(
        &self,
        device_id: &str,
        session: &RemoteSession,
        error: SftpStatusError,
    ) -> ApplicationError {
        match error {
            SftpStatusError::Status(status) => match status.status_code {
                StatusCode::NoSuchFile => ApplicationError::RemoteFileNotFound,
                StatusCode::PermissionDenied => ApplicationError::RemotePermissionDenied,
                _ => ApplicationError::RemoteFilesFailed,
            },
            SftpStatusError::Timeout => {
                self.drop_session_if_closed(device_id, session);
                ApplicationError::RemoteFilesTimedOut
            }
            _ => {
                self.drop_session_if_closed(device_id, session);
                ApplicationError::RemoteFilesFailed
            }
        }
    }

    fn drop_session_if_closed(&self, device_id: &str, session: &RemoteSession) {
        if session.connection.is_closed() {
            self.close_browse_session(device_id);
        }
    }

    /// The open session with `device_id`, opening one if there is none or
    /// the last one has closed.
    async fn remote_session(
        &self,
        device_id: &str,
    ) -> Result<Arc<RemoteSession>, ApplicationError> {
        let connection = self.browse_connection(device_id)?;
        let peer_ip = connection
            .peer_addr
            .ok_or(ApplicationError::DeviceNotConnected)?
            .ip();
        let slot = self
            .browsing
            .slot(device_id)
            .ok_or(ApplicationError::StateUnavailable)?;
        let mut current = slot.lock().await;
        if let Some(session) = current.as_ref()
            && !session.connection.is_closed()
        {
            session.touch();
            return Ok(session.clone());
        }
        *current = None;

        let (port, user, password, roots) = match self.request_offer(device_id, &connection).await?
        {
            SftpReply::Offer {
                port,
                user,
                password,
                roots,
            } => (port, user, password, roots),
            SftpReply::Error(reason) => {
                return Err(ApplicationError::RemoteFilesUnavailable {
                    reason: Some(reason),
                });
            }
            SftpReply::Stopped => return Err(ApplicationError::RemoteFilesFailed),
        };
        let endpoint = SftpEndpoint {
            addr: SocketAddr::new(peer_ip, port),
            user,
            password,
        };
        let sftp_connection = sftp_transport::connect(
            &endpoint,
            self.identity.private_key_der(),
            &connection.certificate_der,
            CONNECT_TIMEOUT,
        )
        .await
        .map_err(|error| {
            if matches!(error, SftpError::HostKeyMismatch) {
                tracing::warn!(device_id, "file server's host key isn't the paired key");
            } else {
                tracing::debug!(device_id, %error, "opening a browse session failed");
            }
            match error {
                SftpError::TimedOut => ApplicationError::RemoteFilesTimedOut,
                SftpError::HostKeyMismatch | SftpError::UnsupportedPeerKey => {
                    ApplicationError::RemoteHostKeyMismatch
                }
                _ => ApplicationError::RemoteFilesFailed,
            }
        })?;
        tracing::debug!(device_id, roots = roots.len(), "browse session opened");

        let session = Arc::new(RemoteSession {
            connection: sftp_connection,
            roots,
            last_used: Mutex::new(Instant::now()),
        });
        // The device may have disconnected meanwhile, dropping this slot;
        // the session then serves this request only.
        if self.browsing.is_current(device_id, &slot) {
            *current = Some(session.clone());
            self.watch_idle_session(Arc::downgrade(&slot), Arc::downgrade(&session));
        }
        Ok(session)
    }

    /// The control connection to a device whose files may be browsed: it
    /// must be paired, connected, and accept `kdeconnect.sftp.request`.
    fn browse_connection(&self, device_id: &str) -> Result<Connection, ApplicationError> {
        let state = self.read_state()?;
        let device = state
            .devices
            .get(device_id)
            .ok_or(ApplicationError::UnknownDevice)?;
        if !device.paired {
            return Err(ApplicationError::NotPaired);
        }
        if !device
            .incoming_capabilities
            .iter()
            .any(|capability| capability == sftp::REQUEST_PACKET_TYPE)
        {
            return Err(ApplicationError::UnsupportedByPeer);
        }
        state
            .connections
            .get(device_id)
            .cloned()
            .ok_or(ApplicationError::DeviceNotConnected)
    }

    /// Ask the peer to serve its files and wait for its answer.
    async fn request_offer(
        &self,
        device_id: &str,
        connection: &Connection,
    ) -> Result<SftpReply, ApplicationError> {
        let (sender, receiver) = oneshot::channel();
        self.browsing
            .offers
            .lock()
            .map_err(|_| ApplicationError::StateUnavailable)?
            .entry(device_id.to_owned())
            .or_default()
            .push(sender);
        let packet =
            sftp::build_request_packet(unix_millis()).map_err(|_| ApplicationError::Internal)?;
        connection
            .packets
            .try_send(packet)
            .map_err(|_| ApplicationError::DeviceNotConnected)?;
        match tokio::time::timeout(OFFER_TIMEOUT, receiver).await {
            Ok(Ok(reply)) => Ok(reply),
            // Waiters are dropped when the device disconnects.
            Ok(Err(_)) => Err(ApplicationError::DeviceNotConnected),
            Err(_) => Err(ApplicationError::RemoteFilesTimedOut),
        }
    }

    /// Handle a `kdeconnect.sftp` packet from a paired peer: hand an offer
    /// or error to whoever is waiting for one, and drop the session when the
    /// peer's server stops.
    pub(super) fn handle_sftp_reply(&self, device_id: &str, body: SftpBody) {
        let Some(reply) = body.reply() else {
            tracing::debug!(device_id, "dropping unrecognized sftp packet");
            return;
        };
        if reply == SftpReply::Stopped {
            self.close_browse_session(device_id);
            return;
        }
        let waiters = self
            .browsing
            .offers
            .lock()
            .ok()
            .and_then(|mut offers| offers.remove(device_id))
            .unwrap_or_default();
        for waiter in waiters {
            let _ = waiter.send(reply.clone());
        }
    }

    /// Forget the device's browse session and fail anyone waiting for an
    /// offer. The connection closes once requests still using the session
    /// finish.
    pub(super) fn close_browse_session(&self, device_id: &str) {
        if let Ok(mut sessions) = self.browsing.sessions.lock() {
            sessions.remove(device_id);
        }
        if let Ok(mut offers) = self.browsing.offers.lock() {
            offers.remove(device_id);
        }
    }

    /// Close every browse session, telling each peer. Called during daemon
    /// shutdown.
    pub async fn shutdown_browsing(&self) {
        self.browsing.cancellation.cancel();
        let slots: Vec<SessionSlot> = match self.browsing.sessions.lock() {
            Ok(mut sessions) => sessions.drain().map(|(_, slot)| slot).collect(),
            Err(_) => return,
        };
        if let Ok(mut offers) = self.browsing.offers.lock() {
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
    fn watch_idle_session(
        &self,
        slot: Weak<tokio::sync::Mutex<Option<Arc<RemoteSession>>>>,
        session: Weak<RemoteSession>,
    ) {
        let cancellation = self.browsing.cancellation.clone();
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

    fn new_transfer(
        &self,
        transfer_id: Uuid,
        device_id: &str,
        direction: TransferDirection,
        file_name: String,
        total: u64,
    ) -> Result<Transfer, ApplicationError> {
        let device_name = self
            .read_state()?
            .devices
            .get(device_id)
            .ok_or(ApplicationError::UnknownDevice)?
            .device_name;
        let now = unix_millis();
        Ok(Transfer::new(TransferSnapshot {
            id: transfer_id,
            device_id: device_id.to_owned(),
            device_name,
            direction,
            status: TransferStatus::Queued,
            file_name,
            total_bytes: total,
            transferred_bytes: 0,
            created_at: now,
            updated_at: now,
            error_code: None,
            saved_path: None,
        }))
    }

    /// Add a transfer resource. Done before its task starts, so the task
    /// always finds it.
    fn insert_transfer(&self, transfer: Transfer) -> Result<(), ApplicationError> {
        let snapshot = transfer.snapshot();
        self.state
            .write()
            .map_err(|_| ApplicationError::StateUnavailable)?
            .transfers
            .insert(snapshot.id, transfer);
        Ok(())
    }

    /// Record a transfer's task so cancellation, disconnect and shutdown can
    /// stop it.
    fn track_transfer_task(
        &self,
        transfer_id: Uuid,
        device_id: &str,
        cancellation: CancellationToken,
        handle: JoinHandle<()>,
    ) {
        let Ok(mut state) = self.state.write() else {
            cancellation.cancel();
            return;
        };
        // A task that already finished has cleaned up after itself.
        if handle.is_finished() {
            return;
        }
        state.transfer_tasks.insert(
            transfer_id,
            TransferTask {
                device_id: device_id.to_owned(),
                cancellation,
                handle,
            },
        );
    }
}

fn normalize_path(path: &str) -> Result<String, ApplicationError> {
    normalize_remote_path(path).map_err(|_| ApplicationError::InvalidRemotePath)
}

fn file_entry(path: String, name: String, metadata: &Metadata) -> FileEntry {
    let kind = match metadata.file_type() {
        FileType::Dir => FileKind::Directory,
        FileType::File => FileKind::File,
        FileType::Symlink => FileKind::Symlink,
        FileType::Other => FileKind::Other,
    };
    FileEntry {
        name,
        path,
        kind,
        size: (kind == FileKind::File).then_some(metadata.size).flatten(),
        modified_at: metadata.mtime.map(|seconds| u64::from(seconds) * 1000),
    }
}
