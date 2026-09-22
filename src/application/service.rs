use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, RwLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use thiserror::Error;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
    time::{Duration, sleep},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    ApplicationEvent, ClipboardSnapshot, Command, EventBus, EventBusError, LocalDeviceSnapshot,
    OperationErrorCode, Pairing, PairingDirection, PairingSnapshot, PairingStatus,
    PairingTransitionError, Query, QueryResult, StatusSnapshot, TransferSnapshot,
};
use crate::device::DeviceRegistry;
use crate::{
    config::{TrustError, TrustStore, TrustedDevice},
    device::DeviceSnapshot,
    protocol::{IdentityBody, Packet, PairingBody, verification_code},
    transport::tls::subject_public_key_info,
};

/// How long a pairing session may remain non-terminal before it expires.
pub const PAIRING_TIMEOUT: Duration = Duration::from_secs(30);

/// Core interface consumed by the local API and future frontends.
pub trait ApplicationService: Send + Sync {
    fn query(&self, query: Query) -> Result<QueryResult, ApplicationError>;
    fn command(&self, command: Command) -> Result<(), ApplicationError>;
    fn subscribe(&self) -> broadcast::Receiver<ApplicationEvent>;
    fn start_outgoing_pairing(&self, device_id: &str) -> Result<PairingSnapshot, ApplicationError>;
    fn accept_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ApplicationError>;
    fn cancel_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, ApplicationError>;
    fn forget_device(&self, device_id: &str) -> Result<(), ApplicationError>;
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
    transfers: BTreeMap<Uuid, TransferSnapshot>,
    clipboard: ClipboardSnapshot,
}

/// Cloneable application facade backed by bounded commands and snapshots.
#[derive(Clone)]
pub struct ApplicationHandle {
    started_at: Instant,
    local_device: LocalDeviceSnapshot,
    protocol_version: u8,
    local_public_key_der: Arc<Vec<u8>>,
    trust_store: Arc<dyn TrustStore + Send + Sync>,
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
        command_capacity: usize,
        event_capacity: usize,
    ) -> Result<(Self, mpsc::Receiver<Command>), ApplicationError> {
        if command_capacity == 0 {
            return Err(ApplicationError::InvalidCommandCapacity);
        }
        let (commands, receiver) = mpsc::channel(command_capacity);
        let events = EventBus::new(event_capacity)?;
        Ok((
            Self {
                started_at: Instant::now(),
                local_device,
                protocol_version,
                local_public_key_der: Arc::new(local_public_key_der),
                trust_store,
                state: Arc::new(RwLock::new(ApplicationState {
                    devices: DeviceRegistry::new(),
                    connections: HashMap::new(),
                    pairings: BTreeMap::new(),
                    pairing_by_device: HashMap::new(),
                    transfers: BTreeMap::new(),
                    clipboard: ClipboardSnapshot {
                        text: String::new(),
                        updated_at: 0,
                        source_device_id: None,
                    },
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
        &self.local_device.device_id
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
                },
            );
        }
        self.mark_device_connected(device_id, observed_at)
    }

    /// Remove a control channel, fail any pairing session in progress on it,
    /// and mark the device unreachable.
    pub fn unregister_connection(&self, device_id: &str) {
        let (had_connection, failed_pairing) = {
            let Ok(mut state) = self.state.write() else {
                return;
            };
            let had_connection = state.connections.remove(device_id).is_some();
            let failed_pairing =
                fail_active_pairing(&mut state, device_id, OperationErrorCode::ConnectionFailed);
            (had_connection, failed_pairing)
        };
        if had_connection {
            let _ = self.mark_device_disconnected(device_id);
        }
        if let Some(snapshot) = failed_pairing {
            let _ = self
                .events
                .publish(super::EventData::PairingUpdated(snapshot));
        }
    }

    /// Remove trust, disconnect, and forget a device entirely.
    pub fn forget_device(&self, device_id: &str) -> Result<(), ApplicationError> {
        let (existed, cancellation, failed_pairing) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| ApplicationError::StateUnavailable)?;
            let cancellation = state
                .connections
                .get(device_id)
                .map(|c| c.cancellation.clone());
            let failed_pairing =
                fail_active_pairing(&mut state, device_id, OperationErrorCode::Internal);
            let existed = state.devices.forget(device_id).is_some();
            state.connections.remove(device_id);
            (existed, cancellation, failed_pairing)
        };
        if !existed {
            return Err(ApplicationError::UnknownDevice);
        }
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

    /// Dispatch a packet received on a registered connection. Only
    /// `kdeconnect.pair` packets are handled here; unpaired devices cannot
    /// trigger any other behavior, and paired-device plugin dispatch is
    /// implemented starting in a later phase.
    pub fn handle_peer_packet(&self, device_id: &str, packet: Packet) {
        if packet.packet_type != "kdeconnect.pair" {
            return;
        }
        let Ok(body) = packet.body_as::<PairingBody>() else {
            return;
        };
        self.handle_pair_body(device_id, body, unix_seconds());
    }

    fn handle_pair_body(&self, device_id: &str, body: PairingBody, received_at: i64) {
        if !body.pair {
            let snapshot = {
                let Ok(mut state) = self.state.write() else {
                    return;
                };
                let Some(&pairing_id) = state.pairing_by_device.get(device_id) else {
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
            return;
        };
        // Reject expired requests and implausible clock skew alike: both
        // manifest as the declared timestamp being outside the pairing
        // window relative to our local clock.
        if (received_at - timestamp).unsigned_abs() > PAIRING_TIMEOUT.as_secs() {
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
                return;
            };
            let Some(connection) = state.connections.get(device_id).cloned() else {
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

    fn status(&self) -> StatusSnapshot {
        StatusSnapshot {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            uptime_seconds: self.started_at.elapsed().as_secs(),
            local_device: self.local_device.clone(),
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
        if query == Query::Status {
            return Ok(QueryResult::Status(self.status()));
        }

        let state = self.read_state()?;
        Ok(match query {
            Query::Status => unreachable!("handled before locking state"),
            Query::Devices => QueryResult::Devices(state.devices.snapshot()),
            Query::Device { device_id } => QueryResult::Device(state.devices.get(&device_id)),
            Query::Pairing { pairing_id } => QueryResult::Pairing(
                state
                    .pairings
                    .get(&pairing_id)
                    .map(|runtime| runtime.pairing.snapshot()),
            ),
            Query::Transfers => QueryResult::Transfers(state.transfers.values().cloned().collect()),
            Query::Transfer { transfer_id } => {
                QueryResult::Transfer(state.transfers.get(&transfer_id).cloned())
            }
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
        ApplicationHandle::new(
            LocalDeviceSnapshot {
                device_id: "local".into(),
                device_name: "MyConnect".into(),
            },
            8,
            b"local-pubkey".to_vec(),
            Arc::new(MemoryTrustStore::default()),
            1,
            1,
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
}
