use std::{
    collections::{BTreeMap, HashMap},
    net::{Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex, RwLock},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use thiserror::Error;
use tokio::{
    sync::{broadcast, mpsc, watch},
    task::JoinHandle,
    time::{Duration, sleep},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{
    CoreEvent, EventBus, EventBusError, LanCommand, LocalDeviceSnapshot, OperationErrorCode,
    Pairing, PairingDirection, PairingSnapshot, PairingStatus, PairingTransitionError,
    StatusSnapshot, TransferSnapshot,
    payload::PayloadPeer,
    plugin::{Capabilities, Plugin, PluginContext, PluginRegistry},
    settings::{Settings, SettingsDefaults, SettingsPatch, SettingsSnapshot},
    transfers::{TransferConfig, Transfers},
};
use crate::device::DeviceRegistry;
use crate::{
    config::{
        LocalIdentity, SettingsError, TrustError, TrustStore, TrustedDevice, TrustedIdentity,
    },
    device::{DeviceReachability, DeviceSnapshot},
    protocol::{DeviceType, IdentityBody, Packet, PairingBody, verification_code},
    transport::tls::subject_public_key_info,
};

/// How long a pairing session may remain non-terminal before it expires.
pub const PAIRING_TIMEOUT: Duration = Duration::from_secs(30);
/// How far an incoming pair request's timestamp may be from our clock, in
/// seconds. Matches KDE Connect's `ALLOWED_TIMESTAMP_TIME_DIFFERENCE_SECONDS`:
/// ordinary clock drift between devices is far larger than the pairing
/// timeout, so the timeout can't double as the skew limit.
pub const PAIRING_TIMESTAMP_TOLERANCE_SECS: u64 = 1800;
/// A live, TLS-authenticated control-channel connection to a peer, as
/// registered by the transport layer once the double identity exchange and
/// TLS handshake succeed.
#[derive(Clone)]
struct Connection {
    packets: mpsc::Sender<Packet>,
    certificate_der: Vec<u8>,
    protocol_version: u8,
    cancellation: CancellationToken,
    /// The peer's IP address, used to dial payload and file-server ports it
    /// advertises on the same host. `None` only if a connection was
    /// registered without going through real LAN transport (e.g. some unit
    /// tests), in which case incoming transfers cannot be established.
    peer_addr: Option<SocketAddr>,
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

struct CoreState {
    devices: DeviceRegistry,
    connections: HashMap<String, Connection>,
    pairings: BTreeMap<Uuid, PairingRuntime>,
    pairing_by_device: HashMap<String, Uuid>,
}

/// The core: devices, connections, pairing, transfers, settings and events,
/// and the plugins built on them. Cheap to clone.
#[derive(Clone)]
pub struct Core {
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
    identity: Arc<LocalIdentity>,
    state: Arc<RwLock<CoreState>>,
    commands: mpsc::Sender<LanCommand>,
    events: EventBus,
    transfers: Transfers,
    plugins: Arc<PluginRegistry>,
}

impl Core {
    /// A core running `plugins`: in the daemon, `plugins::builtin()`, chosen
    /// by [`crate::daemon`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        local_device: LocalDeviceSnapshot,
        protocol_version: u8,
        local_public_key_der: Vec<u8>,
        trust_store: Arc<dyn TrustStore + Send + Sync>,
        plugins: Vec<Arc<dyn Plugin>>,
        command_capacity: usize,
        event_capacity: usize,
        identity: Arc<LocalIdentity>,
        transfer_config: TransferConfig,
    ) -> Result<(Self, mpsc::Receiver<LanCommand>), CoreError> {
        if command_capacity == 0 {
            return Err(CoreError::InvalidCommandCapacity);
        }
        let (commands, receiver) = mpsc::channel(command_capacity);
        let events = EventBus::new(event_capacity)?;
        let devices = paired_devices(trust_store.as_ref());
        let plugins = PluginRegistry::new(plugins);
        let settings = Settings::new(SettingsDefaults {
            device_name: local_device.device_name.clone(),
            download_dir: transfer_config.download_dir.clone(),
        })
        .with_sections(plugins.settings_sections());
        let settings = Arc::new(Mutex::new(settings));
        let transfers = Transfers::new(transfer_config, events.clone(), settings.clone());
        Ok((
            Self {
                started_at: Instant::now(),
                local_device_id: local_device.device_id.into(),
                settings,
                local_device_name: Arc::new(watch::Sender::new(local_device.device_name)),
                protocol_version,
                local_public_key_der: Arc::new(local_public_key_der),
                trust_store,
                identity,
                state: Arc::new(RwLock::new(CoreState {
                    devices,
                    connections: HashMap::new(),
                    pairings: BTreeMap::new(),
                    pairing_by_device: HashMap::new(),
                })),
                commands,
                events,
                transfers,
                plugins: Arc::new(plugins),
            },
            receiver,
        ))
    }

    /// The core as plugins see it.
    pub fn plugin_context(&self) -> PluginContext {
        PluginContext::new(self.clone())
    }

    /// The packet types this core's plugins receive and send, for the LAN
    /// transport to advertise.
    pub fn capabilities(&self) -> Capabilities {
        self.plugins.capabilities()
    }

    pub fn replace_devices(&self, devices: DeviceRegistry) -> Result<(), CoreError> {
        self.state
            .write()
            .map_err(|_| CoreError::StateUnavailable)?
            .devices = devices;
        Ok(())
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.events
    }

    /// Events from now on, for `/events` and other clients.
    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.events.subscribe()
    }

    /// Every plugin's HTTP routes, merged, for the API server.
    pub(crate) fn plugin_routes(&self) -> axum::Router {
        self.plugins.routes(&self.plugin_context())
    }

    /// Every plugin's streaming (upload) routes, merged, for the API server.
    pub(crate) fn plugin_streaming_routes(&self) -> axum::Router {
        self.plugins.streaming_routes(&self.plugin_context())
    }

    /// Every file transfer, whichever feature started it.
    pub fn transfers(&self) -> &Transfers {
        &self.transfers
    }

    /// The device as clients see it, with what plugins add.
    pub fn device(&self, device_id: &str) -> Option<DeviceSnapshot> {
        let device = self.state.read().ok()?.devices.get(device_id)?;
        Some(self.with_plugin_state(device))
    }

    /// Every known device as clients see it: paired ones (even while
    /// offline) and unpaired ones that have announced themselves.
    pub fn devices(&self) -> Result<Vec<DeviceSnapshot>, CoreError> {
        // Plugins add to device snapshots, so those are read out before
        // their state is asked for, never while holding the lock.
        let devices = self.read_state()?.devices.snapshot();
        Ok(devices
            .into_iter()
            .map(|device| self.with_plugin_state(device))
            .collect())
    }

    /// Payload-connection access to a paired, connected device.
    pub(super) fn payload_peer(&self, device_id: &str) -> Result<PayloadPeer, CoreError> {
        let state = self.read_state()?;
        let device = state
            .devices
            .get(device_id)
            .ok_or(CoreError::UnknownDevice)?;
        if !device.paired {
            return Err(CoreError::NotPaired);
        }
        let connection = state
            .connections
            .get(device_id)
            .ok_or(CoreError::DeviceNotConnected)?;
        let config = self.transfers.config();
        Ok(PayloadPeer::new(
            device_id.to_owned(),
            connection.certificate_der.clone(),
            connection.peer_addr.map(|addr| addr.ip()),
            self.identity.clone(),
            config.payload_bind_ip,
            config.payload_connect_timeout,
        ))
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
        *current = settings.with_sections(self.plugins.settings_sections());
        self.apply_settings(&current.snapshot());
    }

    pub fn settings(&self) -> Result<SettingsSnapshot, CoreError> {
        Ok(self
            .settings
            .lock()
            .map_err(|_| CoreError::StateUnavailable)?
            .snapshot())
    }

    /// A plugin's settings section in effect, if it has one.
    pub(super) fn plugin_settings(&self, id: &str) -> Option<serde_json::Value> {
        self.settings.lock().ok()?.section(id)
    }

    /// Validate, persist, and apply a settings change, then publish
    /// `settings.changed` if anything changed. A new device name is
    /// re-announced to the network by the LAN transport.
    pub fn update_settings(&self, patch: SettingsPatch) -> Result<SettingsSnapshot, CoreError> {
        // Holding the lock across the file write keeps concurrent updates
        // from saving out of order.
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| CoreError::StateUnavailable)?;
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
    ) -> Result<DeviceSnapshot, CoreError> {
        let (previous, snapshot) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| CoreError::StateUnavailable)?;
            let previous = state.devices.get(&identity.device_id);
            let snapshot = state
                .devices
                .discover(identity, paired, observed_at)
                .map_err(|_| CoreError::StateUnavailable)?;
            (previous, snapshot)
        };
        let snapshot = self.with_plugin_state(snapshot);
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
    ) -> Result<DeviceSnapshot, CoreError> {
        let snapshot = self
            .state
            .write()
            .map_err(|_| CoreError::StateUnavailable)?
            .devices
            .mark_connected(device_id, observed_at)
            .map_err(|_| CoreError::StateUnavailable)?;
        let snapshot = self.with_plugin_state(snapshot);
        self.events
            .publish(super::EventData::DeviceConnected(snapshot.clone()))?;
        Ok(snapshot)
    }

    pub fn mark_device_disconnected(&self, device_id: &str) -> Result<DeviceSnapshot, CoreError> {
        let snapshot = self
            .state
            .write()
            .map_err(|_| CoreError::StateUnavailable)?
            .devices
            .mark_disconnected(device_id)
            .map_err(|_| CoreError::StateUnavailable)?;
        let snapshot = self.with_plugin_state(snapshot);
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
    ) -> Result<DeviceSnapshot, CoreError> {
        {
            let mut state = self
                .state
                .write()
                .map_err(|_| CoreError::StateUnavailable)?;
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
        self.refresh_trusted_identity(&snapshot);
        self.plugins.connected(&self.plugin_context(), &snapshot);
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
        let (had_connection, failed_pairing) = {
            let Ok(mut state) = self.state.write() else {
                return;
            };
            let had_connection = state.connections.remove(device_id).is_some();
            let failed_pairing =
                fail_active_pairing(&mut state, device_id, OperationErrorCode::ConnectionFailed);
            (had_connection, failed_pairing)
        };
        self.transfers.cancel_device(device_id);
        if had_connection {
            self.plugins.disconnected(&self.plugin_context(), device_id);
            let _ = self.mark_device_disconnected(device_id);
        }
        if let Some(snapshot) = failed_pairing {
            let _ = self
                .events
                .publish(super::EventData::PairingUpdated(snapshot));
        }
    }

    /// Start what plugins run of their own. Called once by the code that
    /// starts the daemon, inside the async runtime.
    pub fn start_plugins(&self) {
        self.plugins.started(&self.plugin_context());
    }

    /// Stop every plugin's own work and close what plugins hold open.
    /// Called during daemon shutdown, after [`Self::shutdown_transfers`].
    pub async fn shutdown_plugins(&self) {
        self.plugins.shutdown().await;
    }

    /// Cancel every in-progress transfer task and wait up to `deadline` for
    /// each to observe cancellation and finish tearing down its own socket
    /// and temporary file, aborting any that do not in time. Called during
    /// daemon shutdown so no transfer task or socket outlives the process.
    pub async fn shutdown_transfers(&self, deadline: Duration) {
        self.transfers.shutdown(deadline).await;
    }

    /// Remove trust, disconnect, and forget a device entirely.
    pub fn forget_device(&self, device_id: &str) -> Result<(), CoreError> {
        let (forgotten, cancellation, failed_pairing) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| CoreError::StateUnavailable)?;
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
            return Err(CoreError::UnknownDevice);
        };
        self.trust_store
            .remove(device_id)
            .map_err(CoreError::Trust)?;
        let ctx = self.plugin_context();
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
            self.plugins.disconnected(&ctx, device_id);
        }
        self.plugins.unpaired(&ctx, device_id);
        if let Some(snapshot) = failed_pairing {
            let _ = self
                .events
                .publish(super::EventData::PairingUpdated(snapshot));
        }
        let _ = self.events.publish(super::EventData::DeviceForgotten(
            self.with_plugin_state(forgotten),
        ));
        Ok(())
    }

    /// Start an outgoing pairing session with a connected device.
    pub fn start_outgoing_pairing(&self, device_id: &str) -> Result<PairingSnapshot, CoreError> {
        let timestamp = unix_seconds();
        let now = unix_millis();
        let (pairing_id, sender, packet) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| CoreError::StateUnavailable)?;
            let device = state
                .devices
                .get(device_id)
                .ok_or(CoreError::UnknownDevice)?;
            if device.paired {
                return Err(CoreError::AlreadyPaired);
            }
            if state.pairing_by_device.contains_key(device_id) {
                return Err(CoreError::PairingInProgress);
            }
            let connection = state
                .connections
                .get(device_id)
                .cloned()
                .ok_or(CoreError::DeviceNotConnected)?;
            let peer_spki = subject_public_key_info(&connection.certificate_der)
                .map_err(|_| CoreError::InvalidPeerCertificate)?;
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
                .map_err(|_| CoreError::StateUnavailable)?;
            state
                .pairing_by_device
                .insert(device_id.to_owned(), pairing_id);
            let body = PairingBody {
                pair: true,
                timestamp: Some(timestamp),
                extra: Default::default(),
            };
            let packet = Packet::from_body(now, "kdeconnect.pair", &body)
                .map_err(|_| CoreError::Internal)?;
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
    pub fn accept_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, CoreError> {
        let (device_id, connection) = {
            let state = self.state.read().map_err(|_| CoreError::StateUnavailable)?;
            let runtime = state
                .pairings
                .get(&pairing_id)
                .ok_or(CoreError::UnknownPairing)?;
            let snapshot = runtime.pairing.snapshot();
            if snapshot.direction != PairingDirection::Incoming {
                return Err(CoreError::InvalidPairingDirection);
            }
            if snapshot.status != PairingStatus::AwaitingConfirmation {
                return Err(CoreError::InvalidPairingState);
            }
            let connection = state
                .connections
                .get(&snapshot.device_id)
                .cloned()
                .ok_or(CoreError::DeviceNotConnected)?;
            (snapshot.device_id, connection)
        };

        self.trust_store
            .put(&TrustedDevice {
                device_id: device_id.clone(),
                certificate_der: connection.certificate_der.clone(),
                last_trusted_protocol_version: connection.protocol_version,
                last_identity: self.known_identity(&device_id),
            })
            .map_err(CoreError::Trust)?;

        let snapshot = {
            let mut state = self
                .state
                .write()
                .map_err(|_| CoreError::StateUnavailable)?;
            let runtime = state
                .pairings
                .get_mut(&pairing_id)
                .ok_or(CoreError::UnknownPairing)?;
            let snapshot = runtime
                .pairing
                .transition(PairingStatus::Accepted, None)
                .map_err(CoreError::InvalidTransition)?;
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
    pub fn cancel_pairing(&self, pairing_id: Uuid) -> Result<PairingSnapshot, CoreError> {
        let (_device_id, sender, snapshot) = {
            let mut state = self
                .state
                .write()
                .map_err(|_| CoreError::StateUnavailable)?;
            let runtime = state
                .pairings
                .get_mut(&pairing_id)
                .ok_or(CoreError::UnknownPairing)?;
            let status = runtime.pairing.snapshot().status;
            if !matches!(
                status,
                PairingStatus::Requested | PairingStatus::AwaitingConfirmation
            ) {
                return Err(CoreError::InvalidPairingState);
            }
            let device_id = runtime.pairing.snapshot().device_id;
            let snapshot = runtime
                .pairing
                .transition(PairingStatus::Rejected, None)
                .map_err(CoreError::InvalidTransition)?;
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
    /// type is routed to the plugin that handles it, only if the sending
    /// device is currently paired; unpaired connections cannot trigger any
    /// other behavior. A type no plugin handles is dropped.
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

        let Some(device) = self
            .state
            .read()
            .ok()
            .and_then(|state| state.devices.get(device_id))
            .filter(|device| device.paired)
        else {
            return;
        };

        if let Some(plugin) = self.plugins.for_packet(&packet.packet_type) {
            plugin.handle_packet(&self.plugin_context(), &device, &packet);
        }
    }

    /// Announce this device on the network now, rather than at the next
    /// interval, so peers answer promptly.
    pub fn announce(&self) -> Result<(), CoreError> {
        self.send_lan_command(LanCommand::AnnounceDiscovery)
    }

    /// Announce this device to one address, for networks where broadcast
    /// discovery doesn't reach the peer. A peer that hears it dials back
    /// over TCP, as it would after a broadcast. Only unicast addresses are
    /// accepted, so this can't be used to spray the identity at a
    /// broadcast or multicast group.
    pub fn announce_to(&self, address: Ipv4Addr) -> Result<(), CoreError> {
        if address.is_unspecified() || address.is_broadcast() || address.is_multicast() {
            return Err(CoreError::InvalidDiscoveryAddress);
        }
        self.send_lan_command(LanCommand::AnnounceTo { address })
    }

    /// Queue `packet` to a paired, connected device. Refused, with a typed
    /// error, unless the device has advertised the packet's type in its
    /// `incomingCapabilities`.
    pub(super) fn send_to_capable(&self, device_id: &str, packet: Packet) -> Result<(), CoreError> {
        let connection = self.capable_connection(device_id, &packet.packet_type)?;
        connection
            .packets
            .try_send(packet)
            .map_err(|_| CoreError::DeviceNotConnected)
    }

    /// Queue `packet` to every paired, connected device that has advertised
    /// its type, except `except`.
    pub(super) fn broadcast_to_capable(&self, packet: &Packet, except: Option<&str>) {
        let Ok(state) = self.state.read() else {
            return;
        };
        for (device_id, connection) in &state.connections {
            if Some(device_id.as_str()) == except {
                continue;
            }
            let accepts = state.devices.get(device_id).is_some_and(|device| {
                device.paired && device.incoming_capabilities.contains(&packet.packet_type)
            });
            if accepts {
                let _ = connection.packets.try_send(packet.clone());
            }
        }
    }

    /// Whether [`Self::send_to_capable`] would take a packet of
    /// `packet_type` for the device now.
    pub(super) fn check_capable(
        &self,
        device_id: &str,
        packet_type: &str,
    ) -> Result<(), CoreError> {
        self.capable_connection(device_id, packet_type).map(|_| ())
    }

    /// The live connection to `device_id`, provided the device is paired
    /// and has advertised `capability` in its `incomingCapabilities`.
    fn capable_connection(
        &self,
        device_id: &str,
        capability: &str,
    ) -> Result<Connection, CoreError> {
        let state = self.read_state()?;
        let device = state
            .devices
            .get(device_id)
            .ok_or(CoreError::UnknownDevice)?;
        if !device.paired {
            return Err(CoreError::NotPaired);
        }
        if !device
            .incoming_capabilities
            .iter()
            .any(|advertised| advertised == capability)
        {
            return Err(CoreError::UnsupportedByPeer);
        }
        state
            .connections
            .get(device_id)
            .cloned()
            .ok_or(CoreError::DeviceNotConnected)
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
        self.plugins.unpaired(&self.plugin_context(), device_id);
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
                last_identity: self.known_identity(&device_id),
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

    fn pairing_snapshot(&self, pairing_id: Uuid) -> Result<PairingSnapshot, CoreError> {
        self.state
            .read()
            .map_err(|_| CoreError::StateUnavailable)?
            .pairings
            .get(&pairing_id)
            .map(|runtime| runtime.pairing.snapshot())
            .ok_or(CoreError::UnknownPairing)
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

    /// How a connected peer currently describes itself, for its trust record.
    fn known_identity(&self, device_id: &str) -> Option<TrustedIdentity> {
        let state = self.state.read().ok()?;
        state
            .devices
            .get(device_id)
            .map(|device| trusted_identity(&device))
    }

    /// Bring a paired peer's trust record up to date with how it describes
    /// itself now, so it is listed that way while offline. Called once a
    /// connection is authenticated, never from an unauthenticated
    /// discovery announcement.
    fn refresh_trusted_identity(&self, device: &DeviceSnapshot) {
        if !device.paired {
            return;
        }
        let Ok(Some(mut trusted)) = self.trust_store.get(&device.device_id) else {
            return;
        };
        let identity = trusted_identity(device);
        if trusted.last_identity.as_ref() == Some(&identity) {
            return;
        }
        trusted.last_identity = Some(identity);
        if let Err(error) = self.trust_store.put(&trusted) {
            tracing::debug!(device_id = device.device_id, %error, "could not update trust record");
        }
    }

    /// Publish `device.updated` with the device's current snapshot.
    pub(super) fn publish_device_update(&self, device_id: &str) {
        let snapshot = self
            .state
            .read()
            .ok()
            .and_then(|state| state.devices.get(device_id));
        if let Some(snapshot) = snapshot {
            let _ = self.events.publish(super::EventData::DeviceUpdated(
                self.with_plugin_state(snapshot),
            ));
        }
    }

    /// `snapshot` as clients see it, with what each plugin adds. Call it
    /// without holding the state lock: it calls into plugins.
    fn with_plugin_state(&self, mut snapshot: DeviceSnapshot) -> DeviceSnapshot {
        snapshot.plugins = self.plugins.device_state(&snapshot.device_id);
        snapshot
    }

    /// Request cancellation of an active transfer, whichever feature
    /// started it. Returns the transfer's current snapshot; its task tears
    /// down its socket and partial file and marks it `cancelled` once it
    /// notices.
    pub fn cancel_transfer(&self, transfer_id: Uuid) -> Result<TransferSnapshot, CoreError> {
        self.transfers.cancel(transfer_id)
    }

    fn send_lan_command(&self, command: LanCommand) -> Result<(), CoreError> {
        self.commands
            .try_send(command)
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::CommandQueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::CommandQueueClosed,
            })
    }

    /// Every pairing this daemon process knows about, including finished
    /// ones.
    pub fn pairings(&self) -> Result<Vec<PairingSnapshot>, CoreError> {
        Ok(self
            .read_state()?
            .pairings
            .values()
            .map(|runtime| runtime.pairing.snapshot())
            .collect())
    }

    /// A pairing this daemon process knows about.
    pub fn pairing(&self, pairing_id: Uuid) -> Option<PairingSnapshot> {
        self.pairing_snapshot(pairing_id).ok()
    }

    pub fn status(&self) -> StatusSnapshot {
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

    fn read_state(&self) -> Result<std::sync::RwLockReadGuard<'_, CoreState>, CoreError> {
        self.state.read().map_err(|_| CoreError::StateUnavailable)
    }
}

fn fail_active_pairing(
    state: &mut CoreState,
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

/// The paired peers in `trust_store`, as unreachable until they are seen.
fn paired_devices(trust_store: &(dyn TrustStore + Send + Sync)) -> DeviceRegistry {
    let mut registry = DeviceRegistry::new();
    match trust_store.list() {
        Ok(devices) => {
            for device in devices {
                registry.restore(paired_device_snapshot(device));
            }
        }
        Err(error) => tracing::warn!(%error, "could not list paired devices"),
    }
    registry
}

fn paired_device_snapshot(device: TrustedDevice) -> DeviceSnapshot {
    // A record from before identities were kept shows its ID until the peer
    // next connects and fills it in.
    let identity = device.last_identity.unwrap_or_else(|| TrustedIdentity {
        device_name: device.device_id.clone(),
        device_type: DeviceType::Phone,
        incoming_capabilities: Vec::new(),
        outgoing_capabilities: Vec::new(),
    });
    DeviceSnapshot {
        device_id: device.device_id,
        device_name: identity.device_name,
        device_type: identity.device_type,
        protocol_version: device.last_trusted_protocol_version,
        incoming_capabilities: identity.incoming_capabilities,
        outgoing_capabilities: identity.outgoing_capabilities,
        reachability: DeviceReachability::Unavailable,
        paired: true,
        pairing: false,
        last_seen_at: 0,
        plugins: Default::default(),
    }
}

fn trusted_identity(device: &DeviceSnapshot) -> TrustedIdentity {
    TrustedIdentity {
        device_name: device.device_name.clone(),
        device_type: device.device_type,
        incoming_capabilities: device.incoming_capabilities.clone(),
        outgoing_capabilities: device.outgoing_capabilities.clone(),
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
pub enum CoreError {
    #[error("command queue capacity must be greater than zero")]
    InvalidCommandCapacity,
    #[error("core command queue is full")]
    CommandQueueFull,
    #[error("core command queue is closed")]
    CommandQueueClosed,
    #[error("core state is unavailable")]
    StateUnavailable,
    #[error("core event bus could not be created")]
    EventBus(#[from] EventBusError),
    #[error("unknown device")]
    UnknownDevice,
    #[error("discovery address must be a unicast IPv4 address")]
    InvalidDiscoveryAddress,
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
    #[error("a plugin's settings section is unknown or its values are invalid")]
    InvalidSettings,
    #[error("transfer is not in a state that allows this operation")]
    InvalidTransferState,
    #[error("internal core error")]
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::testing::{MemoryTrustStore, handle, handle_with_trust, make_identity};

    #[test]
    fn status_and_empty_snapshots_are_readable() {
        let (handle, _commands) = handle();
        assert_eq!(handle.status().protocol_version, 8);
        assert_eq!(handle.devices().unwrap(), Vec::new());
        assert_eq!(handle.pairings().unwrap(), Vec::new());
    }

    #[tokio::test]
    async fn commands_are_bounded_and_observable() {
        let (handle, mut commands) = handle();
        handle.announce().unwrap();
        assert!(matches!(
            handle.announce(),
            Err(CoreError::CommandQueueFull)
        ));
        assert_eq!(commands.recv().await, Some(LanCommand::AnnounceDiscovery));
    }

    #[tokio::test]
    async fn announcing_to_an_address_accepts_only_unicast() {
        let (handle, mut commands) = handle();
        for address in [
            Ipv4Addr::UNSPECIFIED,
            Ipv4Addr::BROADCAST,
            Ipv4Addr::new(224, 0, 0, 251),
        ] {
            assert!(matches!(
                handle.announce_to(address),
                Err(CoreError::InvalidDiscoveryAddress)
            ));
        }
        let address = Ipv4Addr::new(192, 168, 1, 20);
        handle.announce_to(address).unwrap();
        assert_eq!(
            commands.recv().await,
            Some(LanCommand::AnnounceTo { address })
        );
    }

    #[test]
    fn pairing_requires_a_connected_device() {
        let (handle, _commands) = handle();
        assert!(matches!(
            handle.start_outgoing_pairing("missing"),
            Err(CoreError::UnknownDevice)
        ));
    }

    #[test]
    fn paired_devices_are_listed_offline_and_keep_their_latest_identity() {
        let described = "740bd4b9b4184ee497d6caf1da8151be";
        let undescribed = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let trusted = |device_id: &str, last_identity| TrustedDevice {
            device_id: device_id.into(),
            certificate_der: vec![1, 2, 3],
            last_trusted_protocol_version: 8,
            last_identity,
        };
        let (handle, _commands) = handle_with_trust(MemoryTrustStore::new(vec![
            trusted(
                described,
                Some(TrustedIdentity {
                    device_name: "Pixel".into(),
                    device_type: DeviceType::Phone,
                    incoming_capabilities: vec!["kdeconnect.ping".into()],
                    outgoing_capabilities: Vec::new(),
                }),
            ),
            trusted(undescribed, None),
        ]));

        let devices = handle.devices().unwrap();
        let names: Vec<_> = devices.iter().map(|d| d.device_name.as_str()).collect();
        assert_eq!(names, ["Pixel", undescribed]);
        assert!(devices.iter().all(|d| d.paired
            && d.reachability == DeviceReachability::Unavailable
            && d.plugins.is_empty()));
        assert_eq!(devices[0].incoming_capabilities, ["kdeconnect.ping"]);

        let mut identity = make_identity(undescribed, vec!["kdeconnect.share.request".into()]);
        identity.device_name = "Laptop".into();
        identity.device_type = DeviceType::Laptop;
        handle.discover_device(&identity, true, 5).unwrap();
        let (tx, _rx) = mpsc::channel(4);
        handle
            .register_connection(
                undescribed,
                vec![1, 2, 3],
                8,
                tx,
                CancellationToken::new(),
                5,
            )
            .unwrap();
        let stored = handle.trust_store.get(undescribed).unwrap().unwrap();
        assert_eq!(
            stored.last_identity,
            Some(TrustedIdentity {
                device_name: "Laptop".into(),
                device_type: DeviceType::Laptop,
                incoming_capabilities: vec!["kdeconnect.share.request".into()],
                outgoing_capabilities: Vec::new(),
            })
        );

        handle.unregister_connection(undescribed);
        let device = handle.device(undescribed).expect("the device");
        assert_eq!(device.device_name, "Laptop");
        assert_eq!(device.reachability, DeviceReachability::Unavailable);
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
                last_identity: None,
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
}
