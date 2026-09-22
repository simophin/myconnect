//! Peer device state and registry.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::{DeviceType, IdentityBody, IdentityValidationError};

/// Whether a known peer can currently be reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceReachability {
    Discovered,
    Connected,
    Unavailable,
}

/// Immutable, API-facing view of a peer device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSnapshot {
    pub device_id: String,
    pub device_name: String,
    pub device_type: DeviceType,
    pub protocol_version: u8,
    pub incoming_capabilities: Vec<String>,
    pub outgoing_capabilities: Vec<String>,
    pub reachability: DeviceReachability,
    pub paired: bool,
    pub pairing: bool,
    pub last_seen_at: u64,
}

#[derive(Clone, Debug)]
struct DeviceRecord {
    snapshot: DeviceSnapshot,
}

/// In-memory source of truth for peers known during this daemon process.
#[derive(Debug, Default)]
pub struct DeviceRegistry {
    devices: BTreeMap<String, DeviceRecord>,
}

impl DeviceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or refresh a peer from a validated identity announcement.
    pub fn discover(
        &mut self,
        identity: &IdentityBody,
        paired: bool,
        observed_at: u64,
    ) -> Result<DeviceSnapshot, DeviceRegistryError> {
        identity.validate()?;

        let mut incoming_capabilities = identity.incoming_capabilities.clone();
        incoming_capabilities.sort();
        incoming_capabilities.dedup();
        let mut outgoing_capabilities = identity.outgoing_capabilities.clone();
        outgoing_capabilities.sort();
        outgoing_capabilities.dedup();

        let pairing = self
            .devices
            .get(&identity.device_id)
            .is_some_and(|record| record.snapshot.pairing);
        let snapshot = DeviceSnapshot {
            device_id: identity.device_id.clone(),
            device_name: identity.device_name.clone(),
            device_type: identity.device_type,
            protocol_version: identity.protocol_version,
            incoming_capabilities,
            outgoing_capabilities,
            reachability: DeviceReachability::Discovered,
            paired,
            pairing,
            last_seen_at: observed_at,
        };
        self.devices.insert(
            identity.device_id.clone(),
            DeviceRecord {
                snapshot: snapshot.clone(),
            },
        );
        Ok(snapshot)
    }

    pub fn mark_connected(
        &mut self,
        device_id: &str,
        observed_at: u64,
    ) -> Result<DeviceSnapshot, DeviceRegistryError> {
        let record = self.record_mut(device_id)?;
        record.snapshot.reachability = DeviceReachability::Connected;
        record.snapshot.last_seen_at = observed_at;
        Ok(record.snapshot.clone())
    }

    /// Mark a peer unreachable without changing whether its certificate is trusted.
    pub fn mark_disconnected(
        &mut self,
        device_id: &str,
    ) -> Result<DeviceSnapshot, DeviceRegistryError> {
        let record = self.record_mut(device_id)?;
        record.snapshot.reachability = DeviceReachability::Unavailable;
        Ok(record.snapshot.clone())
    }

    pub fn set_paired(
        &mut self,
        device_id: &str,
        paired: bool,
    ) -> Result<DeviceSnapshot, DeviceRegistryError> {
        let record = self.record_mut(device_id)?;
        record.snapshot.paired = paired;
        Ok(record.snapshot.clone())
    }

    pub fn set_pairing(
        &mut self,
        device_id: &str,
        pairing: bool,
    ) -> Result<DeviceSnapshot, DeviceRegistryError> {
        let record = self.record_mut(device_id)?;
        record.snapshot.pairing = pairing;
        Ok(record.snapshot.clone())
    }

    pub fn get(&self, device_id: &str) -> Option<DeviceSnapshot> {
        self.devices
            .get(device_id)
            .map(|record| record.snapshot.clone())
    }

    pub fn snapshot(&self) -> Vec<DeviceSnapshot> {
        self.devices
            .values()
            .map(|record| record.snapshot.clone())
            .collect()
    }

    pub fn forget(&mut self, device_id: &str) -> Option<DeviceSnapshot> {
        self.devices.remove(device_id).map(|record| record.snapshot)
    }

    fn record_mut(&mut self, device_id: &str) -> Result<&mut DeviceRecord, DeviceRegistryError> {
        self.devices
            .get_mut(device_id)
            .ok_or_else(|| DeviceRegistryError::UnknownDevice(device_id.to_owned()))
    }
}

#[derive(Debug, Error)]
pub enum DeviceRegistryError {
    #[error("invalid peer identity")]
    InvalidIdentity(#[from] IdentityValidationError),
    #[error("unknown device {0}")]
    UnknownDevice(String),
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, json};

    use super::*;

    fn identity() -> IdentityBody {
        IdentityBody {
            device_id: "740bd4b9b4184ee497d6caf1da8151be".into(),
            device_name: "FOSS Phone".into(),
            device_type: DeviceType::Phone,
            incoming_capabilities: vec!["clipboard".into(), "clipboard".into()],
            outgoing_capabilities: vec!["share".into()],
            protocol_version: 8,
            extra: Map::new(),
        }
    }

    #[test]
    fn disconnect_preserves_trust_and_updates_reachability() {
        let mut registry = DeviceRegistry::new();
        let identity = identity();
        registry.discover(&identity, true, 10).unwrap();
        registry.mark_connected(&identity.device_id, 11).unwrap();

        let disconnected = registry.mark_disconnected(&identity.device_id).unwrap();

        assert!(disconnected.paired);
        assert_eq!(disconnected.reachability, DeviceReachability::Unavailable);
        assert_eq!(disconnected.last_seen_at, 11);
    }

    #[test]
    fn registry_is_keyed_and_snapshots_are_stably_ordered() {
        let mut registry = DeviceRegistry::new();
        let mut second = identity();
        second.device_id = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();
        registry.discover(&second, false, 1).unwrap();
        let mut first = identity();
        first.device_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
        registry.discover(&first, false, 2).unwrap();

        let snapshot = registry.snapshot();
        assert_eq!(snapshot[0].device_id, first.device_id);
        assert_eq!(snapshot[1].device_id, second.device_id);
        assert_eq!(snapshot[0].incoming_capabilities, ["clipboard"]);
    }

    #[test]
    fn unknown_device_updates_are_typed_errors() {
        assert!(matches!(
            DeviceRegistry::new().mark_disconnected("missing"),
            Err(DeviceRegistryError::UnknownDevice(id)) if id == "missing"
        ));
    }

    #[test]
    fn device_snapshot_uses_stable_api_names() {
        let mut registry = DeviceRegistry::new();
        let snapshot = registry.discover(&identity(), true, 42).unwrap();

        assert_eq!(
            serde_json::to_value(snapshot).unwrap(),
            json!({
                "deviceId": "740bd4b9b4184ee497d6caf1da8151be",
                "deviceName": "FOSS Phone",
                "deviceType": "phone",
                "protocolVersion": 8,
                "incomingCapabilities": ["clipboard"],
                "outgoingCapabilities": ["share"],
                "reachability": "discovered",
                "paired": true,
                "pairing": false,
                "lastSeenAt": 42
            })
        );
    }
}
