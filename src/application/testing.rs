//! A real core for unit tests, of the core and of plugins: an in-memory
//! trust store and clipboard, no LAN transport. Register a connection with
//! an `mpsc` channel to see the packets a device is sent.

use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use super::{ApplicationHandle, Command, LocalDeviceSnapshot, TransferConfig};
use crate::{
    config::{LocalIdentity, TrustError, TrustStore, TrustedDevice},
    protocol::{DeviceType, IdentityBody},
};

#[derive(Default)]
pub(crate) struct MemoryTrustStore(Mutex<Vec<TrustedDevice>>);

impl MemoryTrustStore {
    pub(crate) fn new(devices: Vec<TrustedDevice>) -> Self {
        Self(Mutex::new(devices))
    }
}

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

/// A core with no devices. Its command queue and event bus hold one item
/// each, so tests see overflow early.
pub(crate) fn handle() -> (ApplicationHandle, mpsc::Receiver<Command>) {
    handle_with_trust(MemoryTrustStore::default())
}

pub(crate) fn handle_with_trust(
    trust_store: MemoryTrustStore,
) -> (ApplicationHandle, mpsc::Receiver<Command>) {
    let directory = tempfile::tempdir().unwrap();
    let identity = Arc::new(LocalIdentity::load_or_create(directory.path()).unwrap());
    ApplicationHandle::new(
        LocalDeviceSnapshot {
            device_id: "local".into(),
            device_name: "MyConnect".into(),
        },
        8,
        b"local-pubkey".to_vec(),
        Arc::new(trust_store),
        crate::clipboard::InMemoryClipboard::shared(),
        1,
        1,
        identity,
        TransferConfig::new(directory.path().join("downloads")),
    )
    .unwrap()
}

/// A peer's identity, named "Peer", receiving `incoming_capabilities`.
pub(crate) fn make_identity(device_id: &str, incoming_capabilities: Vec<String>) -> IdentityBody {
    IdentityBody {
        device_id: device_id.to_owned(),
        device_name: "Peer".into(),
        device_type: DeviceType::Phone,
        incoming_capabilities,
        outgoing_capabilities: Vec::new(),
        protocol_version: 8,
        extra: Default::default(),
    }
}
