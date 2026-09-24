use std::{
    collections::{BTreeMap, HashMap},
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use thiserror::Error;
use tokio::{
    sync::{broadcast, mpsc, watch},
    task::JoinHandle,
    time::{Duration, sleep},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    ApplicationEvent, ClipboardSnapshot, Command, EventBus, EventBusError, LocalDeviceSnapshot,
    MAX_CLIPBOARD_TEXT_BYTES, OperationErrorCode, Pairing, PairingDirection, PairingSnapshot,
    PairingStatus, PairingTransitionError, Query, QueryResult, StatusSnapshot, Transfer,
    TransferDirection, TransferSnapshot, TransferStatus,
    settings::{Settings, SettingsDefaults, SettingsPatch, SettingsSnapshot},
    transfer::{TransferConfig, sanitize_file_name, unique_destination},
};
use crate::device::DeviceRegistry;
use crate::{
    clipboard::ClipboardService,
    config::{LocalIdentity, SettingsError, TrustError, TrustStore, TrustedDevice},
    device::DeviceSnapshot,
    plugins::{self, clipboard::ClipboardBody, share},
    protocol::{IdentityBody, Packet, PairingBody, verification_code},
    transport::{
        payload,
        tls::{PeerPin, TlsMaterial, subject_public_key_info},
    },
};

/// How long a pairing session may remain non-terminal before it expires.
pub const PAIRING_TIMEOUT: Duration = Duration::from_secs(30);
/// How far an incoming pair request's timestamp may be from our clock, in
/// seconds. Matches KDE Connect's `ALLOWED_TIMESTAMP_TIME_DIFFERENCE_SECONDS`:
/// ordinary clock drift between devices is far larger than the pairing
/// timeout, so the timeout can't double as the skew limit.
pub const PAIRING_TIMESTAMP_TOLERANCE_SECS: u64 = 1800;
/// Bounded capacity of the channel used to forward HTTP multipart chunks to
/// the outgoing payload connection. Small on purpose: the HTTP handler and
/// the network writer stay coupled by backpressure instead of one side
/// racing ahead and buffering the whole file in memory.
const TRANSFER_CHANNEL_CAPACITY: usize = 4;

/// Minimum gap between `transfer.progress` events for one transfer. The
/// payload loops report every 64 KiB chunk, which on a fast link would flood
/// the bounded event bus and make slow subscribers lag.
const PROGRESS_EVENT_INTERVAL: Duration = Duration::from_millis(100);

/// Records every progress report in the transfer's snapshot but publishes at
/// most one `transfer.progress` event per [`PROGRESS_EVENT_INTERVAL`], plus
/// one for the final byte.
#[derive(Default)]
struct ProgressEvents {
    last_published: Option<Instant>,
}

impl ProgressEvents {
    fn record(&mut self, handle: &ApplicationHandle, transfer_id: Uuid, transferred: u64) {
        let Some(snapshot) = handle.record_transfer_progress(transfer_id, transferred) else {
            return;
        };
        let now = Instant::now();
        let due = self
            .last_published
            .is_none_or(|last| now.duration_since(last) >= PROGRESS_EVENT_INTERVAL);
        if due || snapshot.transferred_bytes == snapshot.total_bytes {
            self.last_published = Some(now);
            let _ = handle
                .events
                .publish(super::EventData::TransferProgress(snapshot));
        }
    }
}

/// Core interface consumed by the local API and future frontends.
pub trait ApplicationService: Send + Sync {
    fn query(&self, query: Query) -> Result<QueryResult, ApplicationError>;
    fn command(&self, command: Command) -> Result<(), ApplicationError>;
    fn subscribe(&self) -> broadcast::Receiver<ApplicationEvent>;
    fn start_outgoing_pairing(&self, device_id: &str) -> Result<PairingSnapshot, ApplicationError>;
    fn accept_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ApplicationError>;
    fn cancel_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ApplicationError>;
    fn forget_device(&self, device_id: &str) -> Result<(), ApplicationError>;
    fn send_ping(&self, device_id: &str, message: Option<String>) -> Result<(), ApplicationError>;
    fn set_clipboard(&self, text: String) -> Result<ClipboardSnapshot, ApplicationError>;
    fn update_settings(&self, patch: SettingsPatch) -> Result<SettingsSnapshot, ApplicationError>;
    fn begin_outgoing_transfer(
        &self,
        device_id: &str,
        file_name: String,
        declared_size: u64,
    ) -> Result<(TransferSnapshot, mpsc::Sender<Bytes>), ApplicationError>;
    fn cancel_transfer(&self, transfer_id: Uuid) -> Result<TransferSnapshot, ApplicationError>;
}

/// A live, TLS-authenticated control-channel connection to a peer, as
/// registered by the transport layer once the double identity exchange and
/// TLS handshake succeed.
#[derive(Clone)]
struct Connection {
    packets: mpsc::Sender<Packet>,
    certificate_der: Vec<u8>,
    protocol_version: u8,
    cancellation: CancellationToken,
    /// The peer's IP address, used to dial its advertised auxiliary payload
    /// port for incoming transfers. `None` only if a connection was
    /// registered without going through real LAN transport (e.g. some unit
    /// tests), in which case incoming transfers cannot be established.
    peer_addr: Option<SocketAddr>,
}

/// A running transfer task's cancellation handle, tracked so cancellation,
/// peer disconnect, and daemon shutdown can all stop it and so it never
/// outlives its resource entry.
struct TransferTask {
    device_id: String,
    cancellation: CancellationToken,
    handle: JoinHandle<()>,
}

struct PairingRuntime {
    pairing: Pairing,
    /// The protocol-level pairing timestamp (Unix seconds) used to compute
    /// the verification code. Distinct from the API-facing millisecond
    /// `createdAt`/`expiresAt` bookkeeping fields.
    #[allow(dead_code)]
    protocol_timestamp: i64,
    timer: Option<JoinHandle<()>>,
}

struct ApplicationState {
    devices: DeviceRegistry,
    connections: HashMap<String, Connection>,
    pairings: BTreeMap<Uuid, PairingRuntime>,
    pairing_by_device: HashMap<String, Uuid>,
    transfers: BTreeMap<Uuid, Transfer>,
    transfer_tasks: HashMap<Uuid, TransferTask>,
    clipboard: ClipboardSnapshot,
    /// Whether clipboard packets are applied from, and sent to, peers at
    /// all. Disabling this only affects network synchronization; the local
    /// snapshot remains readable and writable through the API. Mirrors the
    /// `clipboardSyncEnabled` setting.
    clipboard_sync_enabled: bool,
}

/// Cloneable application facade backed by bounded commands and snapshots.
#[derive(Clone)]
pub struct ApplicationHandle {
    started_at: Instant,
    local_device_id: Arc<str>,
    /// Locked before, never while holding, `state`.
    settings: Arc<Mutex<Settings>>,
    /// The device name in effect, watched by the LAN transport so a rename
    /// reaches peers.
    local_device_name: Arc<watch::Sender<String>>,
    protocol_version: u8,
    local_public_key_der: Arc<Vec<u8>>,
    trust_store: Arc<dyn TrustStore + Send + Sync>,
    clipboard_service: Arc<dyn ClipboardService + Send + Sync>,
    identity: Arc<LocalIdentity>,
    transfer_config: Arc<TransferConfig>,
    state: Arc<RwLock<ApplicationState>>,
    commands: mpsc::Sender<Command>,
    events: EventBus,
}

impl ApplicationHandle {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        local_device: LocalDeviceSnapshot,
        protocol_version: u8,
        local_public_key_der: Vec<u8>,
        trust_store: Arc<dyn TrustStore + Send + Sync>,
        clipboard_service: Arc<dyn ClipboardService + Send + Sync>,
        command_capacity: usize,
        event_capacity: usize,
        identity: Arc<LocalIdentity>,
        transfer_config: TransferConfig,
    ) -> Result<(Self, mpsc::Receiver<Command>), ApplicationError> {
        if command_capacity == 0 {
            return Err(ApplicationError::InvalidCommandCapacity);
        }
        let (commands, receiver) = mpsc::channel(command_capacity);
        let events = EventBus::new(event_capacity)?;
        let settings = Settings::new(SettingsDefaults {
            device_name: local_device.device_name.clone(),
            download_dir: transfer_config.download_dir.clone(),
        });
        Ok((
            Self {
                started_at: Instant::now(),
                local_device_id: local_device.device_id.into(),
                settings: Arc::new(Mutex::new(settings)),
                local_device_name: Arc::new(watch::Sender::new(local_device.device_name)),
                protocol_version,
                local_public_key_der: Arc::new(local_public_key_der),
                trust_store,
                clipboard_service,
                identity,
                transfer_config: Arc::new(transfer_config),
                state: Arc::new(RwLock::new(ApplicationState {
                    devices: DeviceRegistry::new(),
                    connections: HashMap::new(),
                    pairings: BTreeMap::new(),
                    pairing_by_device: HashMap::new(),
                    transfers: BTreeMap::new(),
                    transfer_tasks: HashMap::new(),
                    clipboard: ClipboardSnapshot {
                        text: String::new(),
                        updated_at: 0,
                        source_device_id: None,
                    },
                    clipboard_sync_enabled: true,
                })),
                commands,
                events,
            },
            receiver,
        ))
    }

    pub fn replace_devices(&self, devices: DeviceRegistry) -> Result<(), ApplicationError> {
        self.state
            .write()
            .map_err(|_| ApplicationError::StateUnavailable)?
            .devices = devices;
        Ok(())
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.events
    }

    pub fn local_device_id(&self) -> &str {
        &self.local_device_id
    }

    /// The name this device currently advertises to peers.
    pub fn local_device_name(&self) -> String {
        self.local_device_name.borrow().clone()
    }

    /// Watch the advertised device name, which changes when the user
    /// renames this device.
    pub fn watch_local_device_name(&self) -> watch::Receiver<String> {
        self.local_device_name.subscribe()
    }

    /// Replace the settings this handle was created with, e.g. with ones
    /// backed by the settings file, and apply their runtime effects.
    pub(crate) fn install_settings(&self, settings: Settings) {
        let Ok(mut current) = self.settings.lock() else {
            return;
        };
        *current = settings;
        self.apply_settings(&current.snapshot());
    }

    pub fn settings(&self) -> Result<SettingsSnapshot, ApplicationError> {
        Ok(self
            .settings
            .lock()
            .map_err(|_| ApplicationError::StateUnavailable)?
            .snapshot())
    }

    /// Validate, persist, and apply a settings change, then publish
    /// `settings.changed` if anything changed. A new device name is
    /// re-announced to the network by the LAN transport.
    pub fn update_settings(
        &self,
        patch: SettingsPatch,
    ) -> Result<SettingsSnapshot, ApplicationError> {
        // Holding the lock across the file write keeps concurrent updates
        // from saving out of order.
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| ApplicationError::StateUnavailable)?;
        let before = settings.snapshot();
        let after = settings.update(patch)?;
        if after != before {
            self.apply_settings(&after);
            self.events
                .publish(super::EventData::SettingsChanged(after.clone()))?;
        }
        Ok(after)
    }

    /// Push the settings that live outside [`Settings`] to where they take
    /// effect. The download directory is read when each transfer starts.
    fn apply_settings(&self, settings: &SettingsSnapshot) {
        if let Ok(mut state) = self.state.write() {
            state.clipboard_sync_enabled = settings.clipboard_sync_enabled;
        }
        self.local_device_name.send_if_modified(|name| {
            let changed = *name != settings.device_name;
            if changed {
                name.clone_from(&settings.device_name);
            }
            changed
        });
    }

    pub fn discover_device(
        &self,
        identity: &IdentityBody,
        paired: bool,
        observed_at: u64,
    ) -> Result<DeviceSnapshot, ApplicationError> {
        let (previous, snapshot) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| ApplicationError::StateUnavailable)?;
            let previous = state.devices.get(&identity.device_id);
            let snapshot = state
                .devices
                .discover(identity, paired, observed_at)
                .map_err(|_| ApplicationError::StateUnavailable)?;
            (previous, snapshot)
        };
        let event = if previous.is_none() {
            super::EventData::DeviceDiscovered(snapshot.clone())
        } else {
            super::EventData::DeviceUpdated(snapshot.clone())
        };
        self.events.publish(event)?;
        Ok(snapshot)
    }

    pub fn mark_device_connected(
        &self,
        device_id: &str,
        observed_at: u64,
    ) -> Result<DeviceSnapshot, ApplicationError> {
        let snapshot = self
            .state
            .write()
            .map_err(|_| ApplicationError::StateUnavailable)?
            .devices
            .mark_connected(device_id, observed_at)
            .map_err(|_| ApplicationError::StateUnavailable)?;
        self.events
            .publish(super::EventData::DeviceConnected(snapshot.clone()))?;
        Ok(snapshot)
    }

    pub fn mark_device_disconnected(
        &self,
        device_id: &str,
    ) -> Result<DeviceSnapshot, ApplicationError> {
        let snapshot = self
            .state
            .write()
            .map_err(|_| ApplicationError::StateUnavailable)?
            .devices
            .mark_disconnected(device_id)
            .map_err(|_| ApplicationError::StateUnavailable)?;
        self.events
            .publish(super::EventData::DeviceDisconnected(snapshot.clone()))?;
        Ok(snapshot)
    }

    /// Register a live, TLS-authenticated control channel for `device_id`.
    /// Called by the transport layer only after the pre-TLS identity, the
    /// TLS handshake (with real signature verification and, for trusted
    /// devices, certificate pinning), and the inner post-TLS identity all
    /// agree on the peer's device ID and protocol version.
    pub fn register_connection(
        &self,
        device_id: &str,
        certificate_der: Vec<u8>,
        protocol_version: u8,
        packets: mpsc::Sender<Packet>,
        cancellation: CancellationToken,
        observed_at: u64,
    ) -> Result<DeviceSnapshot, ApplicationError> {
        {
            let mut state = self
                .state
                .write()
                .map_err(|_| ApplicationError::StateUnavailable)?;
            state.connections.insert(
                device_id.to_owned(),
                Connection {
                    packets,
                    certificate_der,
                    protocol_version,
                    cancellation,
                    peer_addr: None,
                },
            );
        }
        let snapshot = self.mark_device_connected(device_id, observed_at)?;
        self.send_clipboard_connect_if_eligible(device_id);
        Ok(snapshot)
    }

    /// Record the peer's IP address for a registered connection, so an
    /// incoming transfer can later dial the auxiliary payload port the peer
    /// advertises on the same host. Called by the transport layer right
    /// after [`Self::register_connection`]; a no-op if the connection has
    /// since been replaced or removed.
    pub fn set_connection_peer_addr(&self, device_id: &str, addr: SocketAddr) {
        if let Ok(mut state) = self.state.write()
            && let Some(connection) = state.connections.get_mut(device_id)
        {
            connection.peer_addr = Some(addr);
        }
    }

    /// Remove a control channel, fail any pairing session in progress on it,
    /// cancel any transfer in progress with it, and mark the device
    /// unreachable.
    pub fn unregister_connection(&self, device_id: &str) {
        let (had_connection, failed_pairing, transfer_cancellations) = {
            let Ok(mut state) = self.state.write() else {
                return;
            };
            let had_connection = state.connections.remove(device_id).is_some();
            let failed_pairing =
                fail_active_pairing(&mut state, device_id, OperationErrorCode::ConnectionFailed);
            let transfer_cancellations: Vec<CancellationToken> = state
                .transfer_tasks
                .values()
                .filter(|task| task.device_id == device_id)
                .map(|task| task.cancellation.clone())
                .collect();
            (had_connection, failed_pairing, transfer_cancellations)
        };
        for cancellation in transfer_cancellations {
            cancellation.cancel();
        }
        if had_connection {
            let _ = self.mark_device_disconnected(device_id);
        }
        if let Some(snapshot) = failed_pairing {
            let _ = self
                .events
                .publish(super::EventData::PairingUpdated(snapshot));
        }
    }

    /// Cancel every in-progress transfer task and wait up to `deadline` for
    /// each to observe cancellation and finish tearing down its own socket
    /// and temporary file, aborting any that do not in time. Called during
    /// daemon shutdown so no transfer task or socket outlives the process.
    pub async fn shutdown_transfers(&self, deadline: Duration) {
        let tasks: Vec<TransferTask> = {
            let Ok(mut state) = self.state.write() else {
                return;
            };
            state.transfer_tasks.drain().map(|(_, task)| task).collect()
        };
        for task in tasks {
            task.cancellation.cancel();
            let abort_handle = task.handle.abort_handle();
            if tokio::time::timeout(deadline, task.handle).await.is_err() {
                abort_handle.abort();
            }
        }
    }

    /// Remove trust, disconnect, and forget a device entirely.
    pub fn forget_device(&self, device_id: &str) -> Result<(), ApplicationError> {
        let (forgotten, cancellation, failed_pairing) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| ApplicationError::StateUnavailable)?;
            let connection = state.connections.get(device_id);
            let cancellation = connection.map(|c| c.cancellation.clone());
            // Tell the peer before the connection closes, so it drops its
            // trust in us too. The transport flushes packets already queued
            // when the connection is cancelled.
            if let Some(connection) = connection {
                let body = PairingBody {
                    pair: false,
                    timestamp: None,
                    extra: Default::default(),
                };
                if let Ok(packet) = Packet::from_body(unix_millis(), "kdeconnect.pair", &body) {
                    let _ = connection.packets.try_send(packet);
                }
            }
            let failed_pairing =
                fail_active_pairing(&mut state, device_id, OperationErrorCode::Internal);
            let forgotten = state.devices.forget(device_id);
            state.connections.remove(device_id);
            (forgotten, cancellation, failed_pairing)
        };
        let Some(forgotten) = forgotten else {
            return Err(ApplicationError::UnknownDevice);
        };
        self.trust_store
            .remove(device_id)
            .map_err(ApplicationError::Trust)?;
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
        }
        if let Some(snapshot) = failed_pairing {
            let _ = self
                .events
                .publish(super::EventData::PairingUpdated(snapshot));
        }
        let _ = self
            .events
            .publish(super::EventData::DeviceForgotten(forgotten));
        Ok(())
    }

    /// Start an outgoing pairing session with a connected device.
    pub fn start_outgoing_pairing(
        &self,
        device_id: &str,
    ) -> Result<PairingSnapshot, ApplicationError> {
        let timestamp = unix_seconds();
        let now = unix_millis();
        let (pairing_id, sender, packet) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| ApplicationError::StateUnavailable)?;
            let device = state
                .devices
                .get(device_id)
                .ok_or(ApplicationError::UnknownDevice)?;
            if device.paired {
                return Err(ApplicationError::AlreadyPaired);
            }
            if state.pairing_by_device.contains_key(device_id) {
                return Err(ApplicationError::PairingInProgress);
            }
            let connection = state
                .connections
                .get(device_id)
                .cloned()
                .ok_or(ApplicationError::DeviceNotConnected)?;
            let peer_spki = subject_public_key_info(&connection.certificate_der)
                .map_err(|_| ApplicationError::InvalidPeerCertificate)?;
            let code = verification_code(&self.local_public_key_der, &peer_spki, timestamp);

            let pairing_id = Uuid::new_v4();
            let snapshot = PairingSnapshot {
                id: pairing_id,
                device_id: device_id.to_owned(),
                device_name: device.device_name.clone(),
                direction: PairingDirection::Outgoing,
                status: PairingStatus::Requested,
                verification_code: Some(code),
                created_at: now,
                expires_at: now.saturating_add(PAIRING_TIMEOUT.as_millis() as u64),
                error_code: None,
            };
            let mut pairing = Pairing::new(snapshot);
            pairing
                .transition(PairingStatus::AwaitingConfirmation, None)
                .expect("requested always allows awaiting_confirmation");

            state
                .devices
                .set_pairing(device_id, true)
                .map_err(|_| ApplicationError::StateUnavailable)?;
            state
                .pairing_by_device
                .insert(device_id.to_owned(), pairing_id);
            let body = PairingBody {
                pair: true,
                timestamp: Some(timestamp),
                extra: Default::default(),
            };
            let packet = Packet::from_body(now, "kdeconnect.pair", &body)
                .map_err(|_| ApplicationError::Internal)?;
            state.pairings.insert(
                pairing_id,
                PairingRuntime {
                    pairing,
                    protocol_timestamp: timestamp,
                    timer: None,
                },
            );
            (pairing_id, connection.packets, packet)
        };

        if sender.try_send(packet).is_err() {
            return Ok(self.fail_pairing(pairing_id, OperationErrorCode::ConnectionFailed));
        }
        self.schedule_pairing_timeout(pairing_id);
        let snapshot = self.pairing_snapshot(pairing_id)?;
        self.events
            .publish(super::EventData::PairingRequested(snapshot.clone()))?;
        Ok(snapshot)
    }

    /// Confirm a locally displayed verification code for an incoming
    /// pairing request. Trust is pinned before the pairing is marked
    /// accepted, and never before this explicit local confirmation.
    pub fn accept_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ApplicationError> {
        let (device_id, connection) = {
            let state = self
                .state
                .read()
                .map_err(|_| ApplicationError::StateUnavailable)?;
            let runtime = state
                .pairings
                .get(&pairing_id)
                .ok_or(ApplicationError::UnknownPairing)?;
            let snapshot = runtime.pairing.snapshot();
            if snapshot.direction != PairingDirection::Incoming {
                return Err(ApplicationError::InvalidPairingDirection);
            }
            if snapshot.status != PairingStatus::AwaitingConfirmation {
                return Err(ApplicationError::InvalidPairingState);
            }
            let connection = state
                .connections
                .get(&snapshot.device_id)
                .cloned()
                .ok_or(ApplicationError::DeviceNotConnected)?;
            (snapshot.device_id, connection)
        };

        self.trust_store
            .put(&TrustedDevice {
                device_id: device_id.clone(),
                certificate_der: connection.certificate_der.clone(),
                last_trusted_protocol_version: connection.protocol_version,
            })
            .map_err(ApplicationError::Trust)?;

        let snapshot = {
            let mut state = self
                .state
                .write()
                .map_err(|_| ApplicationError::StateUnavailable)?;
            let runtime = state
                .pairings
                .get_mut(&pairing_id)
                .ok_or(ApplicationError::UnknownPairing)?;
            let snapshot = runtime
                .pairing
                .transition(PairingStatus::Accepted, None)
                .map_err(ApplicationError::InvalidTransition)?;
            if let Some(timer) = runtime.timer.take() {
                timer.abort();
            }
            state.pairing_by_device.remove(&device_id);
            let _ = state.devices.set_paired(&device_id, true);
            let _ = state.devices.set_pairing(&device_id, false);
            snapshot
        };

        let body = PairingBody {
            pair: true,
            timestamp: None,
            extra: Default::default(),
        };
        if let Ok(packet) = Packet::from_body(unix_millis(), "kdeconnect.pair", &body) {
            let _ = connection.packets.try_send(packet);
        }

        self.events
            .publish(super::EventData::PairingUpdated(snapshot.clone()))?;
        self.publish_device_update(&device_id);
        Ok(snapshot)
    }

    /// Reject an incoming pairing or cancel an outgoing one.
    pub fn cancel_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ApplicationError> {
        let (_device_id, sender, snapshot) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| ApplicationError::StateUnavailable)?;
            let runtime = state
                .pairings
                .get_mut(&pairing_id)
                .ok_or(ApplicationError::UnknownPairing)?;
            let status = runtime.pairing.snapshot().status;
            if !matches!(
                status,
                PairingStatus::Requested | PairingStatus::AwaitingConfirmation
            ) {
                return Err(ApplicationError::InvalidPairingState);
            }
            let device_id = runtime.pairing.snapshot().device_id;
            let snapshot = runtime
                .pairing
                .transition(PairingStatus::Rejected, None)
                .map_err(ApplicationError::InvalidTransition)?;
            if let Some(timer) = runtime.timer.take() {
                timer.abort();
            }
            state.pairing_by_device.remove(&device_id);
            let _ = state.devices.set_pairing(&device_id, false);
            let sender = state.connections.get(&device_id).map(|c| c.packets.clone());
            (device_id, sender, snapshot)
        };

        if let Some(sender) = sender {
            let body = PairingBody {
                pair: false,
                timestamp: None,
                extra: Default::default(),
            };
            if let Ok(packet) = Packet::from_body(unix_millis(), "kdeconnect.pair", &body) {
                let _ = sender.try_send(packet);
            }
        }
        self.events
            .publish(super::EventData::PairingUpdated(snapshot.clone()))?;
        Ok(snapshot)
    }

    /// Dispatch a packet received on a registered connection.
    ///
    /// `kdeconnect.pair` packets are always handled, independent of pairing
    /// state, since pairing itself establishes trust. Every other packet
    /// type is routed to the fixed plugin registry in [`crate::plugins`]
    /// only if the sending device is currently paired; unpaired connections
    /// cannot trigger any other behavior.
    pub fn handle_peer_packet(&self, device_id: &str, packet: Packet) {
        tracing::debug!(device_id, packet_type = %packet.packet_type, "packet received");
        if packet.packet_type == "kdeconnect.pair" {
            let Ok(body) = packet.body_as::<PairingBody>() else {
                tracing::debug!(device_id, "dropping malformed pair packet");
                return;
            };
            self.handle_pair_body(device_id, body, unix_seconds());
            return;
        }

        let paired = self
            .state
            .read()
            .ok()
            .and_then(|state| state.devices.get(device_id))
            .is_some_and(|device| device.paired);
        if !paired {
            return;
        }

        match plugins::dispatch_incoming(&packet) {
            Ok(plugins::IncomingPluginPacket::Clipboard(body)) => {
                self.handle_clipboard(device_id, body, None)
            }
            Ok(plugins::IncomingPluginPacket::ClipboardConnect(body)) => {
                let timestamp = body.timestamp;
                self.handle_clipboard(
                    device_id,
                    ClipboardBody {
                        content: body.content,
                        extra: body.extra,
                    },
                    Some(timestamp),
                )
            }
            Ok(plugins::IncomingPluginPacket::ShareRequest(body)) => {
                self.handle_share_request(device_id, &packet, body)
            }
            // Batch-size updates are modeled for protocol completeness but
            // this build only ever transfers one file per transfer
            // resource, so there is nothing to reconcile them against.
            Ok(plugins::IncomingPluginPacket::ShareRequestUpdate(_)) => {}
            Err(_) => {}
        }
    }

    /// Send a `kdeconnect.ping` packet, optionally carrying a message, to a
    /// paired, connected device. Sending is refused, with a typed error,
    /// unless the device is paired, connected, and has advertised
    /// `kdeconnect.ping` in its `incomingCapabilities`.
    pub fn send_ping(
        &self,
        device_id: &str,
        message: Option<String>,
    ) -> Result<(), ApplicationError> {
        let connection = {
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
                .any(|capability| capability == plugins::ping::PACKET_TYPE)
            {
                return Err(ApplicationError::UnsupportedByPeer);
            }
            state
                .connections
                .get(device_id)
                .cloned()
                .ok_or(ApplicationError::DeviceNotConnected)?
        };

        let packet = plugins::ping::build_packet(unix_millis(), message)
            .map_err(|_| ApplicationError::Internal)?;
        connection
            .packets
            .try_send(packet)
            .map_err(|_| ApplicationError::DeviceNotConnected)
    }

    /// Set the local clipboard text and synchronize it to every paired,
    /// connected peer that advertised the clipboard capability. Rejects
    /// text over [`MAX_CLIPBOARD_TEXT_BYTES`] with a typed error rather than
    /// truncating or accepting it silently. Setting the same text again is
    /// a no-op: no event is published and nothing is resent.
    pub fn set_clipboard(&self, text: String) -> Result<ClipboardSnapshot, ApplicationError> {
        if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
            return Err(ApplicationError::ClipboardTextTooLarge {
                limit: MAX_CLIPBOARD_TEXT_BYTES,
            });
        }

        let snapshot = {
            let mut state = self
                .state
                .write()
                .map_err(|_| ApplicationError::StateUnavailable)?;
            if state.clipboard.text == text {
                return Ok(state.clipboard.clone());
            }
            state.clipboard = ClipboardSnapshot {
                text: text.clone(),
                updated_at: unix_millis(),
                source_device_id: None,
            };
            state.clipboard.clone()
        };

        let _ = self.clipboard_service.set(&text);
        self.events
            .publish(super::EventData::ClipboardChanged(snapshot.clone()))?;
        self.broadcast_clipboard(&text, None);
        Ok(snapshot)
    }

    /// Whether clipboard packets are currently applied from, and sent to,
    /// peers. See [`Self::set_clipboard_sync_enabled`].
    pub fn clipboard_sync_enabled(&self) -> bool {
        self.state
            .read()
            .map(|state| state.clipboard_sync_enabled)
            .unwrap_or(false)
    }

    /// Enable or disable clipboard network synchronization. Disabling this
    /// leaves the local clipboard snapshot readable and writable through
    /// the API; it only stops incoming clipboard packets from being applied
    /// and outgoing ones from being sent.
    pub fn set_clipboard_sync_enabled(&self, enabled: bool) {
        let _ = self.update_settings(SettingsPatch {
            clipboard_sync_enabled: Some(Some(enabled)),
            ..Default::default()
        });
    }

    /// Sync text copied on this machine, as `changes` reports it (see
    /// [`crate::clipboard::SystemClipboard::local_changes`]), the way
    /// [`Self::set_clipboard`] does. Copies made while clipboard sync is off
    /// are dropped rather than read into the snapshot. Runs until `changes`
    /// closes or `shutdown` is cancelled.
    pub fn follow_local_clipboard(
        &self,
        mut changes: watch::Receiver<Option<String>>,
        shutdown: CancellationToken,
    ) -> JoinHandle<()> {
        let application = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => return,
                    changed = changes.changed() => if changed.is_err() {
                        return;
                    },
                }
                let Some(text) = changes.borrow_and_update().clone() else {
                    continue;
                };
                if !application.clipboard_sync_enabled() {
                    continue;
                }
                if let Err(error) = application.set_clipboard(text) {
                    tracing::debug!(%error, "local clipboard change not synced");
                }
            }
        })
    }

    /// Apply a `kdeconnect.clipboard` or `kdeconnect.clipboard.connect` body
    /// received from a paired peer. `timestamp`, present only for the
    /// connect variant, gates staleness: a timestamp that is not strictly
    /// newer than the last known clipboard update is ignored. Duplicate
    /// content is always ignored. This never logs clipboard content, only
    /// its length.
    fn handle_clipboard(&self, device_id: &str, body: ClipboardBody, timestamp: Option<i64>) {
        let content = body.content;
        if content.len() > MAX_CLIPBOARD_TEXT_BYTES {
            tracing::debug!(
                device_id,
                length = content.len(),
                "oversized clipboard packet ignored"
            );
            return;
        }

        let snapshot = {
            let Ok(mut state) = self.state.write() else {
                return;
            };
            if !state.clipboard_sync_enabled {
                return;
            }
            if content == state.clipboard.text {
                // Duplicate: nothing changed, nothing to rebroadcast. This
                // is also part of the feedback-loop guard.
                return;
            }
            if let Some(timestamp) = timestamp
                && timestamp <= state.clipboard.updated_at as i64
            {
                // Stale: the peer's clipboard is not newer than ours.
                return;
            }
            let updated_at = timestamp
                .map(|value| value.max(0) as u64)
                .unwrap_or_else(unix_millis);
            state.clipboard = ClipboardSnapshot {
                text: content.clone(),
                updated_at,
                source_device_id: Some(device_id.to_owned()),
            };
            state.clipboard.clone()
        };

        let _ = self.clipboard_service.set(&content);
        let _ = self
            .events
            .publish(super::EventData::ClipboardChanged(snapshot));
        // Forward to other paired peers, but never back to the device this
        // content was just received from: the core feedback-loop guard.
        self.broadcast_clipboard(&content, Some(device_id));
    }

    /// Send a `kdeconnect.clipboard` packet with `text` to every paired,
    /// connected peer that advertised the clipboard capability, except
    /// `exclude_device_id` (typically the peer `text` was just received
    /// from).
    fn broadcast_clipboard(&self, text: &str, exclude_device_id: Option<&str>) {
        let Ok(state) = self.state.read() else {
            return;
        };
        if !state.clipboard_sync_enabled {
            return;
        }
        for (device_id, connection) in &state.connections {
            if Some(device_id.as_str()) == exclude_device_id {
                continue;
            }
            let Some(device) = state.devices.get(device_id) else {
                continue;
            };
            if !device.paired
                || !device
                    .incoming_capabilities
                    .iter()
                    .any(|capability| capability == plugins::clipboard::PACKET_TYPE)
            {
                continue;
            }
            if let Ok(packet) = plugins::clipboard::build_packet(unix_millis(), text.to_owned()) {
                let _ = connection.packets.try_send(packet);
            }
        }
    }

    /// Send a `kdeconnect.clipboard.connect` packet carrying the current
    /// clipboard text to a newly registered, paired peer that advertised
    /// the clipboard capability, so it can decide whether to adopt our
    /// content under the timestamp rules in [`Self::handle_clipboard`]. A
    /// no-op if clipboard sync is disabled, the peer is unpaired or
    /// unsupported, or the local clipboard is still empty.
    fn send_clipboard_connect_if_eligible(&self, device_id: &str) {
        let Ok(state) = self.state.read() else {
            return;
        };
        if !state.clipboard_sync_enabled || state.clipboard.text.is_empty() {
            return;
        }
        let Some(device) = state.devices.get(device_id) else {
            return;
        };
        if !device.paired
            || !device
                .incoming_capabilities
                .iter()
                .any(|capability| capability == plugins::clipboard::PACKET_TYPE)
        {
            return;
        }
        let Some(connection) = state.connections.get(device_id) else {
            return;
        };
        if let Ok(packet) = plugins::clipboard::build_connect_packet(
            unix_millis(),
            state.clipboard.text.clone(),
            state.clipboard.updated_at as i64,
        ) {
            let _ = connection.packets.try_send(packet);
        }
    }

    fn handle_pair_body(&self, device_id: &str, body: PairingBody, received_at: i64) {
        if !body.pair {
            let snapshot = {
                let Ok(mut state) = self.state.write() else {
                    return;
                };
                let Some(&pairing_id) = state.pairing_by_device.get(device_id) else {
                    drop(state);
                    self.handle_peer_unpair(device_id);
                    return;
                };
                let Some(runtime) = state.pairings.get_mut(&pairing_id) else {
                    return;
                };
                let Ok(snapshot) = runtime.pairing.transition(PairingStatus::Rejected, None) else {
                    return;
                };
                if let Some(timer) = runtime.timer.take() {
                    timer.abort();
                }
                state.pairing_by_device.remove(device_id);
                let _ = state.devices.set_pairing(device_id, false);
                snapshot
            };
            let _ = self
                .events
                .publish(super::EventData::PairingUpdated(snapshot));
            return;
        }

        enum Outcome {
            ConfirmOutgoing(Uuid),
            NewIncoming,
            Ignore,
        }

        let outcome = {
            let Ok(state) = self.state.read() else {
                return;
            };
            match state.pairing_by_device.get(device_id) {
                Some(&pairing_id) => {
                    let runtime = state.pairings.get(&pairing_id);
                    match runtime.map(|runtime| runtime.pairing.snapshot()) {
                        Some(snapshot)
                            if snapshot.direction == PairingDirection::Outgoing
                                && snapshot.status == PairingStatus::AwaitingConfirmation =>
                        {
                            Outcome::ConfirmOutgoing(pairing_id)
                        }
                        _ => Outcome::Ignore,
                    }
                }
                None => Outcome::NewIncoming,
            }
        };

        match outcome {
            Outcome::ConfirmOutgoing(pairing_id) => self.confirm_outgoing_pairing(pairing_id),
            Outcome::NewIncoming => self.begin_incoming_pairing(device_id, body, received_at),
            Outcome::Ignore => {}
        }
    }

    /// Handle `kdeconnect.pair {"pair": false}` outside a pairing session:
    /// the peer has unpaired us. Remove its trust and mark it unpaired, but
    /// keep the connection open, as KDE Connect does, so the device stays
    /// reachable and can be paired again.
    fn handle_peer_unpair(&self, device_id: &str) {
        let was_paired = self
            .state
            .read()
            .ok()
            .and_then(|state| state.devices.get(device_id))
            .is_some_and(|device| device.paired);
        let was_trusted = self.trust_store.remove(device_id).unwrap_or(false);
        if !was_paired && !was_trusted {
            return;
        }
        if let Ok(mut state) = self.state.write() {
            let _ = state.devices.set_paired(device_id, false);
        }
        self.publish_device_update(device_id);
    }

    fn confirm_outgoing_pairing(&self, pairing_id: Uuid) {
        let Some((device_id, certificate_der, protocol_version)) = ({
            let Ok(state) = self.state.read() else {
                return;
            };
            state.pairings.get(&pairing_id).map(|runtime| {
                let snapshot = runtime.pairing.snapshot();
                let connection = state.connections.get(&snapshot.device_id);
                (
                    snapshot.device_id,
                    connection
                        .map(|c| c.certificate_der.clone())
                        .unwrap_or_default(),
                    connection.map(|c| c.protocol_version).unwrap_or(0),
                )
            })
        }) else {
            return;
        };
        if certificate_der.is_empty() {
            return;
        }

        if self
            .trust_store
            .put(&TrustedDevice {
                device_id: device_id.clone(),
                certificate_der,
                last_trusted_protocol_version: protocol_version,
            })
            .is_err()
        {
            let snapshot = self.fail_pairing(pairing_id, OperationErrorCode::Internal);
            let _ = self
                .events
                .publish(super::EventData::PairingUpdated(snapshot));
            return;
        }

        let snapshot = {
            let Ok(mut state) = self.state.write() else {
                return;
            };
            let Some(runtime) = state.pairings.get_mut(&pairing_id) else {
                return;
            };
            let Ok(snapshot) = runtime.pairing.transition(PairingStatus::Accepted, None) else {
                return;
            };
            if let Some(timer) = runtime.timer.take() {
                timer.abort();
            }
            state.pairing_by_device.remove(&device_id);
            let _ = state.devices.set_paired(&device_id, true);
            let _ = state.devices.set_pairing(&device_id, false);
            snapshot
        };
        let _ = self
            .events
            .publish(super::EventData::PairingUpdated(snapshot));
        self.publish_device_update(&device_id);
    }

    fn begin_incoming_pairing(&self, device_id: &str, body: PairingBody, received_at: i64) {
        let Some(timestamp) = body.timestamp else {
            tracing::debug!(device_id, "dropping pair request without a timestamp");
            return;
        };
        // Reject stale requests and implausible clock skew alike, as KDE
        // Connect does ("Device clocks are out of sync").
        if (received_at - timestamp).unsigned_abs() > PAIRING_TIMESTAMP_TOLERANCE_SECS {
            tracing::debug!(
                device_id,
                skew_secs = received_at - timestamp,
                "dropping pair request: device clocks are out of sync"
            );
            return;
        }

        let snapshot = {
            let Ok(mut state) = self.state.write() else {
                return;
            };
            if state.pairing_by_device.contains_key(device_id) {
                return;
            }
            let Some(device) = state.devices.get(device_id) else {
                tracing::debug!(device_id, "dropping pair request from an unknown device");
                return;
            };
            let Some(connection) = state.connections.get(device_id).cloned() else {
                tracing::debug!(device_id, "dropping pair request without a connection");
                return;
            };
            let Ok(peer_spki) = subject_public_key_info(&connection.certificate_der) else {
                return;
            };
            let code = verification_code(&self.local_public_key_der, &peer_spki, timestamp);
            let now = unix_millis();
            let pairing_id = Uuid::new_v4();
            let pairing_snapshot = PairingSnapshot {
                id: pairing_id,
                device_id: device_id.to_owned(),
                device_name: device.device_name.clone(),
                direction: PairingDirection::Incoming,
                status: PairingStatus::Requested,
                verification_code: Some(code),
                created_at: now,
                expires_at: now.saturating_add(PAIRING_TIMEOUT.as_millis() as u64),
                error_code: None,
            };
            let mut pairing = Pairing::new(pairing_snapshot);
            let awaiting_snapshot = pairing
                .transition(PairingStatus::AwaitingConfirmation, None)
                .expect("requested always allows awaiting_confirmation");
            let _ = state.devices.set_pairing(device_id, true);
            state
                .pairing_by_device
                .insert(device_id.to_owned(), pairing_id);
            state.pairings.insert(
                pairing_id,
                PairingRuntime {
                    pairing,
                    protocol_timestamp: timestamp,
                    timer: None,
                },
            );
            (pairing_id, awaiting_snapshot)
        };
        let (pairing_id, pairing_snapshot) = snapshot;
        self.schedule_pairing_timeout(pairing_id);
        let _ = self
            .events
            .publish(super::EventData::PairingRequested(pairing_snapshot));
    }

    fn fail_pairing(&self, pairing_id: Uuid, error_code: OperationErrorCode) -> PairingSnapshot {
        let outcome = (|| {
            let mut state = self.state.write().ok()?;
            let runtime = state.pairings.get_mut(&pairing_id)?;
            let device_id = runtime.pairing.snapshot().device_id;
            let snapshot = runtime
                .pairing
                .transition(PairingStatus::Failed, Some(error_code))
                .ok()?;
            if let Some(timer) = runtime.timer.take() {
                timer.abort();
            }
            state.pairing_by_device.remove(&device_id);
            let _ = state.devices.set_pairing(&device_id, false);
            Some(snapshot)
        })();
        outcome.unwrap_or_else(|| {
            self.pairing_snapshot(pairing_id)
                .unwrap_or(PairingSnapshot {
                    id: pairing_id,
                    device_id: String::new(),
                    device_name: String::new(),
                    direction: PairingDirection::Outgoing,
                    status: PairingStatus::Failed,
                    verification_code: None,
                    created_at: 0,
                    expires_at: 0,
                    error_code: Some(error_code),
                })
        })
    }

    fn pairing_snapshot(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ApplicationError> {
        self.state
            .read()
            .map_err(|_| ApplicationError::StateUnavailable)?
            .pairings
            .get(&pairing_id)
            .map(|runtime| runtime.pairing.snapshot())
            .ok_or(ApplicationError::UnknownPairing)
    }

    fn schedule_pairing_timeout(&self, pairing_id: Uuid) {
        let handle = self.clone();
        let task = tokio::spawn(async move {
            sleep(PAIRING_TIMEOUT).await;
            handle.expire_pairing(pairing_id);
        });
        let Ok(mut state) = self.state.write() else {
            task.abort();
            return;
        };
        match state.pairings.get_mut(&pairing_id) {
            Some(runtime) => runtime.timer = Some(task),
            None => task.abort(),
        }
    }

    fn expire_pairing(&self, pairing_id: Uuid) {
        let snapshot = {
            let Ok(mut state) = self.state.write() else {
                return;
            };
            let Some(runtime) = state.pairings.get_mut(&pairing_id) else {
                return;
            };
            let status = runtime.pairing.snapshot().status;
            if !matches!(
                status,
                PairingStatus::Requested | PairingStatus::AwaitingConfirmation
            ) {
                return;
            }
            let device_id = runtime.pairing.snapshot().device_id;
            let Ok(snapshot) = runtime
                .pairing
                .transition(PairingStatus::Expired, Some(OperationErrorCode::TimedOut))
            else {
                return;
            };
            // This task is the timer being cleared; nothing left to abort.
            runtime.timer = None;
            state.pairing_by_device.remove(&device_id);
            let _ = state.devices.set_pairing(&device_id, false);
            snapshot
        };
        let _ = self
            .events
            .publish(super::EventData::PairingUpdated(snapshot));
    }

    fn publish_device_update(&self, device_id: &str) {
        if let Ok(state) = self.state.read()
            && let Some(snapshot) = state.devices.get(device_id)
        {
            let _ = self
                .events
                .publish(super::EventData::DeviceUpdated(snapshot));
        }
    }

    // --- File transfer ---------------------------------------------------

    /// Start sending a file to a paired, connected device that has
    /// advertised the share capability. Returns immediately with a `queued`
    /// transfer resource and a bounded sender the caller (the local HTTP
    /// API) streams file bytes into; a background task drains that channel
    /// into a fresh auxiliary TLS payload connection to the peer, so the
    /// whole path from the HTTP request body to the network never holds
    /// more than a few chunks in memory at once.
    pub fn begin_outgoing_transfer(
        &self,
        device_id: &str,
        file_name: String,
        declared_size: u64,
    ) -> Result<(TransferSnapshot, mpsc::Sender<Bytes>), ApplicationError> {
        if file_name.trim().is_empty() {
            return Err(ApplicationError::InvalidFileName);
        }
        if declared_size > self.transfer_config.max_transfer_bytes {
            return Err(ApplicationError::TransferTooLarge {
                limit: self.transfer_config.max_transfer_bytes,
            });
        }

        let (connection, device_name) = {
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
                .any(|capability| capability == share::PACKET_TYPE)
            {
                return Err(ApplicationError::UnsupportedByPeer);
            }
            let connection = state
                .connections
                .get(device_id)
                .cloned()
                .ok_or(ApplicationError::DeviceNotConnected)?;
            (connection, device.device_name.clone())
        };

        let now = unix_millis();
        let transfer_id = Uuid::new_v4();
        let transfer = Transfer::new(TransferSnapshot {
            id: transfer_id,
            device_id: device_id.to_owned(),
            device_name,
            direction: TransferDirection::Outgoing,
            status: TransferStatus::Queued,
            file_name,
            total_bytes: declared_size,
            transferred_bytes: 0,
            created_at: now,
            updated_at: now,
            error_code: None,
            saved_path: None,
        });
        let started = transfer.snapshot();

        let (chunk_tx, chunk_rx) = mpsc::channel::<Bytes>(TRANSFER_CHANNEL_CAPACITY);
        let cancellation = CancellationToken::new();
        let task_handle = self.clone();
        let task_cancellation = cancellation.clone();
        let device_id_owned = device_id.to_owned();
        let join = tokio::spawn(async move {
            task_handle
                .run_outgoing_transfer(
                    transfer_id,
                    device_id_owned,
                    connection,
                    chunk_rx,
                    task_cancellation,
                )
                .await;
        });

        {
            let mut state = self
                .state
                .write()
                .map_err(|_| ApplicationError::StateUnavailable)?;
            state.transfers.insert(transfer_id, transfer);
            state.transfer_tasks.insert(
                transfer_id,
                TransferTask {
                    device_id: device_id.to_owned(),
                    cancellation,
                    handle: join,
                },
            );
        }
        self.events
            .publish(super::EventData::TransferStarted(started.clone()))?;
        Ok((started, chunk_tx))
    }

    async fn run_outgoing_transfer(
        &self,
        transfer_id: Uuid,
        device_id: String,
        connection: Connection,
        mut chunk_rx: mpsc::Receiver<Bytes>,
        cancellation: CancellationToken,
    ) {
        let Some(total) = self.transfer_snapshot(transfer_id).map(|s| s.total_bytes) else {
            self.cleanup_transfer_task(transfer_id);
            return;
        };
        if self
            .transition_transfer(transfer_id, TransferStatus::Connecting, None)
            .is_none()
        {
            self.cleanup_transfer_task(transfer_id);
            return;
        }

        let (listener, port) = match payload::bind_payload_listener(
            self.transfer_config.payload_bind_ip,
            crate::transport::lan::TCP_PORT_RANGE,
        )
        .await
        {
            Ok(bound) => bound,
            Err(_) => {
                self.fail_transfer(transfer_id, OperationErrorCode::ConnectionFailed);
                self.cleanup_transfer_task(transfer_id);
                return;
            }
        };

        let file_name = self
            .transfer_snapshot(transfer_id)
            .map(|s| s.file_name)
            .unwrap_or_default();
        let packet = match share::build_request_packet(unix_millis(), file_name, None, total, port)
        {
            Ok(packet) => packet,
            Err(_) => {
                self.fail_transfer(transfer_id, OperationErrorCode::Internal);
                self.cleanup_transfer_task(transfer_id);
                return;
            }
        };
        if connection.packets.try_send(packet).is_err() {
            self.fail_transfer(transfer_id, OperationErrorCode::ConnectionFailed);
            self.cleanup_transfer_task(transfer_id);
            return;
        }

        let material = TlsMaterial::new(
            self.identity.certificate_der(),
            self.identity.private_key_der(),
        );
        let accept = payload::accept_payload_connection(
            listener,
            self.transfer_config.payload_connect_timeout,
            &material,
            &device_id,
            PeerPin::Pinned(connection.certificate_der.clone()),
        );
        let mut stream = tokio::select! {
            _ = cancellation.cancelled() => {
                self.finish_transfer_cancelled(transfer_id);
                self.cleanup_transfer_task(transfer_id);
                return;
            }
            result = accept => match result {
                Ok(stream) => stream,
                Err(_) => {
                    self.fail_transfer(transfer_id, OperationErrorCode::ConnectionFailed);
                    self.cleanup_transfer_task(transfer_id);
                    return;
                }
            },
        };

        if self
            .transition_transfer(transfer_id, TransferStatus::Transferring, None)
            .is_none()
        {
            self.cleanup_transfer_task(transfer_id);
            return;
        }

        let mut progress = ProgressEvents::default();
        let result = payload::forward_channel(
            &mut chunk_rx,
            &mut stream,
            total,
            &cancellation,
            |transferred| progress.record(self, transfer_id, transferred),
        )
        .await;
        match result {
            Ok(()) => self.complete_transfer(transfer_id, None),
            Err(payload::PayloadError::Cancelled) => self.finish_transfer_cancelled(transfer_id),
            Err(_) => self.fail_transfer(transfer_id, OperationErrorCode::ConnectionFailed),
        }
        self.cleanup_transfer_task(transfer_id);
    }

    /// Handle an incoming `kdeconnect.share.request` from a paired peer:
    /// validate and sanitize the declared filename and size, then, if
    /// acceptable, spawn a task that dials the peer's advertised payload
    /// port and streams the file into a temporary file, finalizing only on
    /// full, verified completion.
    fn handle_share_request(
        &self,
        device_id: &str,
        packet: &Packet,
        body: share::ShareRequestBody,
    ) {
        let Some(total) = packet
            .payload_size
            .filter(|size| *size >= 0)
            .map(|size| size as u64)
        else {
            return;
        };
        let Some(port) = packet
            .payload_transfer_info
            .as_ref()
            .and_then(share::payload_port)
        else {
            return;
        };

        let Some((device_name, peer_ip, certificate_der)) = ({
            let Ok(state) = self.state.read() else {
                return;
            };
            let device = state.devices.get(device_id);
            let connection = state.connections.get(device_id);
            match (device, connection) {
                (Some(device), Some(connection)) => connection.peer_addr.map(|addr| {
                    (
                        device.device_name,
                        addr.ip(),
                        connection.certificate_der.clone(),
                    )
                }),
                _ => None,
            }
        }) else {
            return;
        };

        let now = unix_millis();
        let transfer_id = Uuid::new_v4();
        let sanitized = sanitize_file_name(&body.filename);
        let rejection = if sanitized.is_err() {
            Some(OperationErrorCode::ProtocolError)
        } else if total > self.transfer_config.max_transfer_bytes {
            Some(OperationErrorCode::Unavailable)
        } else {
            None
        };

        let display_name = sanitized.clone().unwrap_or_else(|_| body.filename.clone());
        let transfer = Transfer::new(TransferSnapshot {
            id: transfer_id,
            device_id: device_id.to_owned(),
            device_name,
            direction: TransferDirection::Incoming,
            status: TransferStatus::Queued,
            file_name: display_name,
            total_bytes: total,
            transferred_bytes: 0,
            created_at: now,
            updated_at: now,
            error_code: None,
            saved_path: None,
        });
        let started = transfer.snapshot();

        if let Some(error_code) = rejection {
            let Ok(mut state) = self.state.write() else {
                return;
            };
            state.transfers.insert(transfer_id, transfer);
            drop(state);
            let _ = self
                .events
                .publish(super::EventData::TransferStarted(started));
            self.fail_transfer(transfer_id, error_code);
            self.cleanup_transfer_task(transfer_id);
            return;
        }
        let file_name = sanitized.expect("rejection handled above");

        let cancellation = CancellationToken::new();
        let task_handle = self.clone();
        let task_cancellation = cancellation.clone();
        let device_id_owned = device_id.to_owned();
        let addr = SocketAddr::new(peer_ip, port);
        let join = tokio::spawn(async move {
            task_handle
                .run_incoming_transfer(
                    transfer_id,
                    device_id_owned,
                    addr,
                    certificate_der,
                    file_name,
                    total,
                    task_cancellation,
                )
                .await;
        });

        let Ok(mut state) = self.state.write() else {
            join.abort();
            return;
        };
        state.transfers.insert(transfer_id, transfer);
        state.transfer_tasks.insert(
            transfer_id,
            TransferTask {
                device_id: device_id.to_owned(),
                cancellation,
                handle: join,
            },
        );
        drop(state);
        let _ = self
            .events
            .publish(super::EventData::TransferStarted(started));
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_incoming_transfer(
        &self,
        transfer_id: Uuid,
        device_id: String,
        addr: SocketAddr,
        peer_certificate_der: Vec<u8>,
        file_name: String,
        total: u64,
        cancellation: CancellationToken,
    ) {
        if self
            .transition_transfer(transfer_id, TransferStatus::Connecting, None)
            .is_none()
        {
            self.cleanup_transfer_task(transfer_id);
            return;
        }

        let material = TlsMaterial::new(
            self.identity.certificate_der(),
            self.identity.private_key_der(),
        );
        let connect = payload::connect_payload(
            addr,
            self.transfer_config.payload_connect_timeout,
            &material,
            &device_id,
            PeerPin::Pinned(peer_certificate_der),
        );
        let mut stream = tokio::select! {
            _ = cancellation.cancelled() => {
                self.finish_transfer_cancelled(transfer_id);
                self.cleanup_transfer_task(transfer_id);
                return;
            }
            result = connect => match result {
                Ok(stream) => stream,
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
            &mut stream,
            &mut file,
            total,
            &cancellation,
            |transferred| progress.record(self, transfer_id, transferred),
        )
        .await;
        drop(file);

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

    /// Request cancellation of an active transfer. Returns the resource's
    /// current snapshot immediately; the actual socket/file teardown and
    /// the transition to `cancelled` happen asynchronously in the transfer
    /// task once it observes the cancellation, since the task alone owns
    /// the socket and temporary file involved.
    pub fn cancel_transfer(&self, transfer_id: Uuid) -> Result<TransferSnapshot, ApplicationError> {
        let snapshot = self
            .transfer_snapshot(transfer_id)
            .ok_or(ApplicationError::UnknownTransfer)?;
        if matches!(
            snapshot.status,
            TransferStatus::Completed | TransferStatus::Cancelled | TransferStatus::Failed
        ) {
            return Err(ApplicationError::InvalidTransferState);
        }
        if let Ok(state) = self.state.read()
            && let Some(task) = state.transfer_tasks.get(&transfer_id)
        {
            task.cancellation.cancel();
        }
        Ok(snapshot)
    }

    fn transfer_snapshot(&self, transfer_id: Uuid) -> Option<TransferSnapshot> {
        self.state
            .read()
            .ok()?
            .transfers
            .get(&transfer_id)
            .map(|transfer| transfer.snapshot())
    }

    fn transition_transfer(
        &self,
        transfer_id: Uuid,
        next: TransferStatus,
        error_code: Option<OperationErrorCode>,
    ) -> Option<TransferSnapshot> {
        let mut state = self.state.write().ok()?;
        let transfer = state.transfers.get_mut(&transfer_id)?;
        transfer.transition(next, unix_millis(), error_code).ok()
    }

    /// Record progress in the transfer's snapshot, and return that snapshot
    /// for publishing.
    fn record_transfer_progress(
        &self,
        transfer_id: Uuid,
        transferred_bytes: u64,
    ) -> Option<TransferSnapshot> {
        let mut state = self.state.write().ok()?;
        let transfer = state.transfers.get_mut(&transfer_id)?;
        transfer
            .record_progress(transferred_bytes, unix_millis())
            .ok()
    }

    fn complete_transfer(&self, transfer_id: Uuid, saved_path: Option<PathBuf>) {
        let snapshot = (|| {
            let mut state = self.state.write().ok()?;
            let transfer = state.transfers.get_mut(&transfer_id)?;
            transfer.complete(saved_path, unix_millis()).ok()
        })();
        if let Some(snapshot) = snapshot {
            let _ = self
                .events
                .publish(super::EventData::TransferCompleted(snapshot));
        }
    }

    /// There is no dedicated `transfer.cancelled` SSE event in the fixed
    /// event-type set; a cancellation is reported through the same
    /// `transfer.failed` event name, carrying a snapshot whose `status`
    /// field is `cancelled` (not `failed`) and no error code, so SSE
    /// watchers still learn the terminal state and clients distinguish the
    /// two by the snapshot's `status`, not the event name.
    fn finish_transfer_cancelled(&self, transfer_id: Uuid) {
        if let Some(snapshot) =
            self.transition_transfer(transfer_id, TransferStatus::Cancelled, None)
        {
            let _ = self
                .events
                .publish(super::EventData::TransferFailed(snapshot));
        }
    }

    fn fail_transfer(&self, transfer_id: Uuid, error_code: OperationErrorCode) {
        if let Some(snapshot) =
            self.transition_transfer(transfer_id, TransferStatus::Failed, Some(error_code))
        {
            let _ = self
                .events
                .publish(super::EventData::TransferFailed(snapshot));
        }
    }

    fn cleanup_transfer_task(&self, transfer_id: Uuid) {
        if let Ok(mut state) = self.state.write() {
            state.transfer_tasks.remove(&transfer_id);
        }
    }

    fn status(&self) -> StatusSnapshot {
        StatusSnapshot {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            uptime_seconds: self.started_at.elapsed().as_secs(),
            local_device: LocalDeviceSnapshot {
                device_id: self.local_device_id.to_string(),
                device_name: self.local_device_name(),
            },
            protocol_version: self.protocol_version,
        }
    }

    fn read_state(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, ApplicationState>, ApplicationError> {
        self.state
            .read()
            .map_err(|_| ApplicationError::StateUnavailable)
    }
}

fn fail_active_pairing(
    state: &mut ApplicationState,
    device_id: &str,
    error_code: OperationErrorCode,
) -> Option<PairingSnapshot> {
    let pairing_id = state.pairing_by_device.remove(device_id)?;
    let runtime = state.pairings.get_mut(&pairing_id)?;
    let snapshot = runtime
        .pairing
        .transition(PairingStatus::Failed, Some(error_code))
        .ok()?;
    if let Some(timer) = runtime.timer.take() {
        timer.abort();
    }
    let _ = state.devices.set_pairing(device_id, false);
    Some(snapshot)
}

impl ApplicationService for ApplicationHandle {
    fn query(&self, query: Query) -> Result<QueryResult, ApplicationError> {
        match query {
            Query::Status => return Ok(QueryResult::Status(self.status())),
            Query::Settings => return Ok(QueryResult::Settings(self.settings()?)),
            _ => {}
        }

        let state = self.read_state()?;
        Ok(match query {
            Query::Status | Query::Settings => unreachable!("handled before locking state"),
            Query::Devices => QueryResult::Devices(state.devices.snapshot()),
            Query::Device { device_id } => QueryResult::Device(state.devices.get(&device_id)),
            Query::Pairings => QueryResult::Pairings(
                state
                    .pairings
                    .values()
                    .map(|runtime| runtime.pairing.snapshot())
                    .collect(),
            ),
            Query::Pairing { pairing_id } => QueryResult::Pairing(
                state
                    .pairings
                    .get(&pairing_id)
                    .map(|runtime| runtime.pairing.snapshot()),
            ),
            Query::Transfers => QueryResult::Transfers(
                state
                    .transfers
                    .values()
                    .map(|transfer| transfer.snapshot())
                    .collect(),
            ),
            Query::Transfer { transfer_id } => QueryResult::Transfer(
                state
                    .transfers
                    .get(&transfer_id)
                    .map(|transfer| transfer.snapshot()),
            ),
            Query::Clipboard => QueryResult::Clipboard(state.clipboard.clone()),
        })
    }

    fn command(&self, command: Command) -> Result<(), ApplicationError> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => ApplicationError::CommandQueueFull,
                mpsc::error::TrySendError::Closed(_) => ApplicationError::CommandQueueClosed,
            })
    }

    fn subscribe(&self) -> broadcast::Receiver<ApplicationEvent> {
        self.events.subscribe()
    }

    fn start_outgoing_pairing(&self, device_id: &str) -> Result<PairingSnapshot, ApplicationError> {
        ApplicationHandle::start_outgoing_pairing(self, device_id)
    }

    fn accept_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ApplicationError> {
        ApplicationHandle::accept_pairing(self, pairing_id)
    }

    fn cancel_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ApplicationError> {
        ApplicationHandle::cancel_pairing(self, pairing_id)
    }

    fn forget_device(&self, device_id: &str) -> Result<(), ApplicationError> {
        ApplicationHandle::forget_device(self, device_id)
    }

    fn send_ping(&self, device_id: &str, message: Option<String>) -> Result<(), ApplicationError> {
        ApplicationHandle::send_ping(self, device_id, message)
    }

    fn set_clipboard(&self, text: String) -> Result<ClipboardSnapshot, ApplicationError> {
        ApplicationHandle::set_clipboard(self, text)
    }

    fn update_settings(&self, patch: SettingsPatch) -> Result<SettingsSnapshot, ApplicationError> {
        ApplicationHandle::update_settings(self, patch)
    }

    fn begin_outgoing_transfer(
        &self,
        device_id: &str,
        file_name: String,
        declared_size: u64,
    ) -> Result<(TransferSnapshot, mpsc::Sender<Bytes>), ApplicationError> {
        ApplicationHandle::begin_outgoing_transfer(self, device_id, file_name, declared_size)
    }

    fn cancel_transfer(&self, transfer_id: Uuid) -> Result<TransferSnapshot, ApplicationError> {
        ApplicationHandle::cancel_transfer(self, transfer_id)
    }
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("command queue capacity must be greater than zero")]
    InvalidCommandCapacity,
    #[error("application command queue is full")]
    CommandQueueFull,
    #[error("application command queue is closed")]
    CommandQueueClosed,
    #[error("application state is unavailable")]
    StateUnavailable,
    #[error("application event bus could not be created")]
    EventBus(#[from] EventBusError),
    #[error("application returned an unexpected query result")]
    UnexpectedQueryResult,
    #[error("unknown device")]
    UnknownDevice,
    #[error("device is already paired")]
    AlreadyPaired,
    #[error("a pairing session is already in progress for this device")]
    PairingInProgress,
    #[error("device does not have a live connection")]
    DeviceNotConnected,
    #[error("device is not paired")]
    NotPaired,
    #[error("peer has not advertised support for this packet type")]
    UnsupportedByPeer,
    #[error("peer certificate is invalid")]
    InvalidPeerCertificate,
    #[error("unknown pairing")]
    UnknownPairing,
    #[error("this pairing direction does not accept local confirmation")]
    InvalidPairingDirection,
    #[error("pairing is not in a state that allows this operation")]
    InvalidPairingState,
    #[error("invalid pairing transition")]
    InvalidTransition(#[from] PairingTransitionError),
    #[error("trust store operation failed")]
    Trust(#[source] TrustError),
    #[error("clipboard text exceeds the {limit}-byte limit")]
    ClipboardTextTooLarge { limit: usize },
    #[error("file name must not be empty")]
    InvalidFileName,
    #[error("declared transfer size exceeds the {limit}-byte limit")]
    TransferTooLarge { limit: u64 },
    #[error("unknown transfer")]
    UnknownTransfer,
    #[error("device name must be 1 to 32 characters without reserved punctuation")]
    InvalidDeviceName,
    #[error("download directory must be an absolute path that can be created")]
    InvalidDownloadDir,
    #[error("settings could not be saved")]
    Settings(#[source] SettingsError),
    #[error("transfer is not in a state that allows this operation")]
    InvalidTransferState,
    #[error("internal application error")]
    Internal,
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::config::TrustedDevice;

    #[derive(Default)]
    struct MemoryTrustStore(Mutex<Vec<TrustedDevice>>);

    impl TrustStore for MemoryTrustStore {
        fn get(&self, device_id: &str) -> Result<Option<TrustedDevice>, TrustError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.device_id == device_id)
                .cloned())
        }
        fn list(&self) -> Result<Vec<TrustedDevice>, TrustError> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn put(&self, device: &TrustedDevice) -> Result<(), TrustError> {
            let mut devices = self.0.lock().unwrap();
            devices.retain(|d| d.device_id != device.device_id);
            devices.push(device.clone());
            Ok(())
        }
        fn remove(&self, device_id: &str) -> Result<bool, TrustError> {
            let mut devices = self.0.lock().unwrap();
            let before = devices.len();
            devices.retain(|d| d.device_id != device_id);
            Ok(devices.len() != before)
        }
    }

    fn handle() -> (ApplicationHandle, mpsc::Receiver<Command>) {
        let directory = tempfile::tempdir().unwrap();
        let identity = Arc::new(LocalIdentity::load_or_create(directory.path()).unwrap());
        ApplicationHandle::new(
            LocalDeviceSnapshot {
                device_id: "local".into(),
                device_name: "MyConnect".into(),
            },
            8,
            b"local-pubkey".to_vec(),
            Arc::new(MemoryTrustStore::default()),
            crate::clipboard::InMemoryClipboard::shared(),
            1,
            1,
            identity,
            TransferConfig::new(directory.path().join("downloads")),
        )
        .unwrap()
    }

    #[test]
    fn status_and_empty_snapshots_are_queryable() {
        let (handle, _commands) = handle();
        assert!(matches!(
            handle.query(Query::Status).unwrap(),
            QueryResult::Status(StatusSnapshot {
                protocol_version: 8,
                ..
            })
        ));
        assert_eq!(
            handle.query(Query::Devices).unwrap(),
            QueryResult::Devices(Vec::new())
        );
    }

    #[tokio::test]
    async fn commands_are_bounded_and_observable() {
        let (handle, mut commands) = handle();
        handle.command(Command::AnnounceDiscovery).unwrap();
        assert!(matches!(
            handle.command(Command::AnnounceDiscovery),
            Err(ApplicationError::CommandQueueFull)
        ));
        assert_eq!(commands.recv().await, Some(Command::AnnounceDiscovery));
    }

    #[test]
    fn pairing_requires_a_connected_device() {
        let (handle, _commands) = handle();
        assert!(matches!(
            handle.start_outgoing_pairing("missing"),
            Err(ApplicationError::UnknownDevice)
        ));
    }

    fn make_identity(device_id: &str, incoming_capabilities: Vec<String>) -> IdentityBody {
        IdentityBody {
            device_id: device_id.to_owned(),
            device_name: "Peer".into(),
            device_type: crate::protocol::DeviceType::Phone,
            incoming_capabilities,
            outgoing_capabilities: Vec::new(),
            protocol_version: 8,
            extra: Default::default(),
        }
    }

    #[test]
    fn unpaired_devices_cannot_be_pinged() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        let identity = make_identity(device_id, vec![plugins::ping::PACKET_TYPE.into()]);
        handle.discover_device(&identity, false, 1).unwrap();
        let (tx, _rx) = mpsc::channel(4);
        handle
            .register_connection(device_id, vec![1, 2, 3], 8, tx, CancellationToken::new(), 1)
            .unwrap();

        // Sending is refused before pairing, with a typed error rather than
        // a silent no-op.
        assert!(matches!(
            handle.send_ping(device_id, None),
            Err(ApplicationError::NotPaired)
        ));
    }

    #[test]
    fn capability_filtering_rejects_unsupported_peers_and_paired_devices_can_be_pinged() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        let identity = make_identity(device_id, Vec::new());
        handle.discover_device(&identity, true, 1).unwrap();
        let (tx, mut rx) = mpsc::channel(4);
        handle
            .register_connection(device_id, vec![1, 2, 3], 8, tx, CancellationToken::new(), 1)
            .unwrap();

        // Paired and connected, but the peer never advertised the ping
        // capability: sending must be refused with a typed rejection, not a
        // silent no-op or a panic.
        assert!(matches!(
            handle.send_ping(device_id, None),
            Err(ApplicationError::UnsupportedByPeer)
        ));

        // The peer re-announces (e.g. on reconnect) advertising the
        // capability.
        let identity_with_ping = make_identity(device_id, vec![plugins::ping::PACKET_TYPE.into()]);
        handle
            .discover_device(&identity_with_ping, true, 2)
            .unwrap();

        handle.send_ping(device_id, Some("hello".into())).unwrap();
        let sent = rx.try_recv().unwrap();
        assert_eq!(sent.packet_type, plugins::ping::PACKET_TYPE);
        let body: plugins::ping::PingBody = sent.body_as().unwrap();
        assert_eq!(body.message.as_deref(), Some("hello"));

        // Incoming pings are not handled yet: one from a paired peer is
        // dropped without disturbing the connection.
        let incoming = plugins::ping::build_packet(2_u64, Some("pong".into())).unwrap();
        handle.handle_peer_packet(device_id, incoming);
        handle.send_ping(device_id, None).unwrap();
        assert_eq!(
            rx.try_recv().unwrap().packet_type,
            plugins::ping::PACKET_TYPE
        );
    }

    fn unpair_packet() -> Packet {
        let body = PairingBody {
            pair: false,
            timestamp: None,
            extra: Default::default(),
        };
        Packet::from_body(1, "kdeconnect.pair", &body).unwrap()
    }

    #[test]
    fn forgetting_a_connected_device_tells_the_peer_before_disconnecting() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        handle
            .discover_device(&make_identity(device_id, Vec::new()), true, 1)
            .unwrap();
        let (tx, mut rx) = mpsc::channel(4);
        let cancellation = CancellationToken::new();
        handle
            .register_connection(device_id, vec![1, 2, 3], 8, tx, cancellation.clone(), 1)
            .unwrap();

        handle.forget_device(device_id).unwrap();

        let sent = rx.try_recv().unwrap();
        assert_eq!(sent.packet_type, "kdeconnect.pair");
        let body: PairingBody = sent.body_as().unwrap();
        assert!(!body.pair);
        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn unpair_from_a_paired_peer_removes_trust_and_keeps_the_connection() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        handle
            .trust_store
            .put(&TrustedDevice {
                device_id: device_id.into(),
                certificate_der: vec![1, 2, 3],
                last_trusted_protocol_version: 8,
            })
            .unwrap();
        handle
            .discover_device(&make_identity(device_id, Vec::new()), true, 1)
            .unwrap();
        let (tx, _rx) = mpsc::channel(4);
        let cancellation = CancellationToken::new();
        handle
            .register_connection(device_id, vec![1, 2, 3], 8, tx, cancellation.clone(), 1)
            .unwrap();
        let mut events = handle.subscribe();

        handle.handle_peer_packet(device_id, unpair_packet());

        assert!(handle.trust_store.get(device_id).unwrap().is_none());
        match events.try_recv().unwrap().event {
            super::super::EventData::DeviceUpdated(device) => {
                assert!(!device.paired);
                assert_eq!(
                    device.reachability,
                    crate::device::DeviceReachability::Connected
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
        assert!(!cancellation.is_cancelled());
    }

    #[test]
    fn unpair_from_an_unpaired_peer_is_ignored() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        handle
            .discover_device(&make_identity(device_id, Vec::new()), false, 1)
            .unwrap();
        let mut events = handle.subscribe();

        handle.handle_peer_packet(device_id, unpair_packet());

        assert!(events.try_recv().is_err());
    }

    fn connect_paired_clipboard_peer(
        handle: &ApplicationHandle,
        device_id: &str,
    ) -> mpsc::Receiver<Packet> {
        let identity = make_identity(
            device_id,
            vec![
                plugins::clipboard::PACKET_TYPE.into(),
                plugins::clipboard::CONNECT_PACKET_TYPE.into(),
            ],
        );
        handle.discover_device(&identity, true, 1).unwrap();
        let (tx, rx) = mpsc::channel(4);
        handle
            .register_connection(device_id, vec![1, 2, 3], 8, tx, CancellationToken::new(), 1)
            .unwrap();
        rx
    }

    #[test]
    fn local_set_clipboard_updates_snapshot_and_sends_to_paired_peers() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        // Empty clipboard: connect-time sync is skipped.
        let mut rx = connect_paired_clipboard_peer(&handle, device_id);
        assert!(rx.try_recv().is_err());

        let snapshot = handle.set_clipboard("hello".into()).unwrap();
        assert_eq!(snapshot.text, "hello");
        assert_eq!(snapshot.source_device_id, None);

        let sent = rx.try_recv().unwrap();
        assert_eq!(sent.packet_type, plugins::clipboard::PACKET_TYPE);
        let body: plugins::clipboard::ClipboardBody = sent.body_as().unwrap();
        assert_eq!(body.content, "hello");

        match handle.query(Query::Clipboard).unwrap() {
            QueryResult::Clipboard(clipboard) => assert_eq!(clipboard.text, "hello"),
            other => panic!("unexpected query result: {other:?}"),
        }
    }

    #[test]
    fn setting_identical_clipboard_text_is_a_no_op() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        let mut rx = connect_paired_clipboard_peer(&handle, device_id);

        handle.set_clipboard("hello".into()).unwrap();
        rx.try_recv().unwrap();

        handle.set_clipboard("hello".into()).unwrap();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn oversized_clipboard_text_is_rejected() {
        let (handle, _commands) = handle();
        let oversized = "x".repeat(MAX_CLIPBOARD_TEXT_BYTES + 1);
        assert!(matches!(
            handle.set_clipboard(oversized),
            Err(ApplicationError::ClipboardTextTooLarge { limit }) if limit == MAX_CLIPBOARD_TEXT_BYTES
        ));
    }

    #[test]
    fn remote_clipboard_update_is_applied_and_forwarded_but_not_echoed_back() {
        let (handle, _commands) = handle();
        let sender_id = "740bd4b9b4184ee497d6caf1da8151be";
        let other_id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let mut sender_rx = connect_paired_clipboard_peer(&handle, sender_id);
        let mut other_rx = connect_paired_clipboard_peer(&handle, other_id);

        let incoming = plugins::clipboard::build_packet(1_u64, "from peer".into()).unwrap();
        handle.handle_peer_packet(sender_id, incoming);

        match handle.query(Query::Clipboard).unwrap() {
            QueryResult::Clipboard(clipboard) => {
                assert_eq!(clipboard.text, "from peer");
                assert_eq!(clipboard.source_device_id.as_deref(), Some(sender_id));
            }
            other => panic!("unexpected query result: {other:?}"),
        }

        // The feedback-loop guard: content just received from a peer must
        // never be sent straight back to that same peer.
        assert!(sender_rx.try_recv().is_err());

        // But it is forwarded to other paired, capable, connected peers.
        let forwarded = other_rx.try_recv().unwrap();
        let body: plugins::clipboard::ClipboardBody = forwarded.body_as().unwrap();
        assert_eq!(body.content, "from peer");
    }

    #[test]
    fn duplicate_remote_clipboard_content_is_ignored() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        handle.set_clipboard("hello".into()).unwrap();
        let mut rx = connect_paired_clipboard_peer(&handle, device_id);
        // Connect-time sync fires because the peer is eligible and our
        // clipboard is non-empty.
        rx.try_recv().unwrap();

        let mut events = handle.subscribe();
        let duplicate = plugins::clipboard::build_packet(1_u64, "hello".into()).unwrap();
        handle.handle_peer_packet(device_id, duplicate);

        // No rebroadcast and no event for content that already matches.
        assert!(rx.try_recv().is_err());
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn stale_clipboard_connect_timestamp_is_ignored() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        handle.set_clipboard("newer".into()).unwrap();
        let clipboard = match handle.query(Query::Clipboard).unwrap() {
            QueryResult::Clipboard(clipboard) => clipboard,
            other => panic!("unexpected query result: {other:?}"),
        };

        let identity = make_identity(
            device_id,
            vec![
                plugins::clipboard::PACKET_TYPE.into(),
                plugins::clipboard::CONNECT_PACKET_TYPE.into(),
            ],
        );
        handle.discover_device(&identity, true, 1).unwrap();
        let (tx, _rx) = mpsc::channel(4);
        handle
            .register_connection(device_id, vec![1, 2, 3], 8, tx, CancellationToken::new(), 1)
            .unwrap();

        let stale = plugins::clipboard::build_connect_packet(
            1_u64,
            "older".into(),
            clipboard.updated_at as i64 - 1000,
        )
        .unwrap();
        handle.handle_peer_packet(device_id, stale);

        match handle.query(Query::Clipboard).unwrap() {
            QueryResult::Clipboard(current) => assert_eq!(current.text, "newer"),
            other => panic!("unexpected query result: {other:?}"),
        }
    }

    #[test]
    fn disabling_clipboard_sync_stops_apply_and_send() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        let mut rx = connect_paired_clipboard_peer(&handle, device_id);
        handle.set_clipboard_sync_enabled(false);
        assert!(!handle.clipboard_sync_enabled());

        // Outgoing: a local set no longer reaches the peer...
        handle.set_clipboard("local only".into()).unwrap();
        assert!(rx.try_recv().is_err());

        // ...and incoming packets are not applied.
        let incoming = plugins::clipboard::build_packet(1_u64, "from peer".into()).unwrap();
        handle.handle_peer_packet(device_id, incoming);
        match handle.query(Query::Clipboard).unwrap() {
            QueryResult::Clipboard(clipboard) => assert_eq!(clipboard.text, "local only"),
            other => panic!("unexpected query result: {other:?}"),
        }

        // ...but the local snapshot is still readable and writable.
        handle.set_clipboard_sync_enabled(true);
        handle.set_clipboard("resumed".into()).unwrap();
        let sent = rx.try_recv().unwrap();
        assert_eq!(sent.packet_type, plugins::clipboard::PACKET_TYPE);
    }

    #[tokio::test]
    async fn local_clipboard_changes_are_synced_while_sync_is_enabled() {
        let (handle, _commands) = handle();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        let mut rx = connect_paired_clipboard_peer(&handle, device_id);
        let (changes, receiver) = watch::channel(None);
        let shutdown = CancellationToken::new();
        let follower = handle.follow_local_clipboard(receiver, shutdown.clone());

        changes.send_replace(Some("copied".into()));
        let sent = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let body: plugins::clipboard::ClipboardBody = sent.body_as().unwrap();
        assert_eq!(body.content, "copied");

        handle.set_clipboard_sync_enabled(false);
        changes.send_replace(Some("private".into()));
        sleep(Duration::from_millis(50)).await;
        match handle.query(Query::Clipboard).unwrap() {
            QueryResult::Clipboard(clipboard) => assert_eq!(clipboard.text, "copied"),
            other => panic!("unexpected query result: {other:?}"),
        }
        assert!(rx.try_recv().is_err());

        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), follower)
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn progress_events_are_throttled_but_the_final_byte_is_published() {
        let (handle, _commands) = handle();
        let transfer_id = Uuid::new_v4();
        let mut transfer = Transfer::new(TransferSnapshot {
            id: transfer_id,
            device_id: "peer".into(),
            device_name: "Peer".into(),
            direction: TransferDirection::Incoming,
            status: TransferStatus::Queued,
            file_name: "file.bin".into(),
            total_bytes: 100,
            transferred_bytes: 0,
            created_at: 1,
            updated_at: 1,
            error_code: None,
            saved_path: None,
        });
        transfer
            .transition(TransferStatus::Transferring, 1, None)
            .unwrap();
        handle
            .state
            .write()
            .unwrap()
            .transfers
            .insert(transfer_id, transfer);
        let mut events = handle.subscribe();

        let mut progress = ProgressEvents::default();
        let mut published = Vec::new();
        for transferred in 1..=100 {
            progress.record(&handle, transfer_id, transferred);
            // The test bus holds one event, so drain it after every report.
            while let Ok(event) = events.try_recv() {
                match event.event {
                    super::super::EventData::TransferProgress(snapshot) => {
                        published.push(snapshot.transferred_bytes);
                    }
                    other => panic!("unexpected event: {other:?}"),
                }
            }
        }

        assert_eq!(published, vec![1, 100]);
        assert_eq!(
            handle
                .transfer_snapshot(transfer_id)
                .unwrap()
                .transferred_bytes,
            100
        );
    }
}
