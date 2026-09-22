use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::device::DeviceSnapshot;

/// Public, non-sensitive failure categories safe to return to API clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationErrorCode {
    ConnectionFailed,
    ProtocolError,
    TimedOut,
    Unavailable,
    Internal,
}

/// Summary of the daemon's local KDE Connect identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDeviceSnapshot {
    pub device_id: String,
    pub device_name: String,
}

/// Immutable status response assembled by the application layer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusSnapshot {
    pub version: String,
    pub uptime_seconds: u64,
    pub local_device: LocalDeviceSnapshot,
    pub protocol_version: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingDirection {
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PairingStatus {
    Requested,
    AwaitingConfirmation,
    Accepted,
    Rejected,
    Expired,
    Failed,
}

impl PairingStatus {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Requested,
                Self::AwaitingConfirmation | Self::Rejected | Self::Expired | Self::Failed
            ) | (
                Self::AwaitingConfirmation,
                Self::Accepted | Self::Rejected | Self::Expired | Self::Failed
            )
        )
    }
}

/// Immutable view of an incoming or outgoing pairing operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingSnapshot {
    pub id: Uuid,
    pub device_id: String,
    pub device_name: String,
    pub direction: PairingDirection,
    pub status: PairingStatus,
    pub verification_code: Option<String>,
    pub created_at: u64,
    pub expires_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<OperationErrorCode>,
}

/// Mutable core representation of a pairing operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pairing {
    snapshot: PairingSnapshot,
}

impl Pairing {
    pub fn new(snapshot: PairingSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn snapshot(&self) -> PairingSnapshot {
        self.snapshot.clone()
    }

    pub fn transition(
        &mut self,
        next: PairingStatus,
        error_code: Option<OperationErrorCode>,
    ) -> Result<PairingSnapshot, PairingTransitionError> {
        let current = self.snapshot.status;
        if !current.can_transition_to(next) {
            return Err(PairingTransitionError { current, next });
        }
        self.snapshot.status = next;
        self.snapshot.error_code = error_code;
        Ok(self.snapshot())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("invalid pairing transition from {current:?} to {next:?}")]
pub struct PairingTransitionError {
    pub current: PairingStatus,
    pub next: PairingStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Queued,
    Connecting,
    Transferring,
    Completed,
    Cancelled,
    Failed,
}

impl TransferStatus {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Queued,
                Self::Connecting | Self::Transferring | Self::Cancelled | Self::Failed
            ) | (
                Self::Connecting,
                Self::Transferring | Self::Cancelled | Self::Failed
            ) | (
                Self::Transferring,
                Self::Completed | Self::Cancelled | Self::Failed
            )
        )
    }
}

/// Immutable view of a file transfer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSnapshot {
    pub id: Uuid,
    pub device_id: String,
    pub device_name: String,
    pub direction: TransferDirection,
    pub status: TransferStatus,
    pub file_name: String,
    pub total_bytes: u64,
    pub transferred_bytes: u64,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<OperationErrorCode>,
}

/// Mutable core representation of a transfer operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    snapshot: TransferSnapshot,
}

impl Transfer {
    pub fn new(snapshot: TransferSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn snapshot(&self) -> TransferSnapshot {
        self.snapshot.clone()
    }

    pub fn transition(
        &mut self,
        next: TransferStatus,
        updated_at: u64,
        error_code: Option<OperationErrorCode>,
    ) -> Result<TransferSnapshot, TransferTransitionError> {
        let current = self.snapshot.status;
        if !current.can_transition_to(next) {
            return Err(TransferTransitionError { current, next });
        }
        self.snapshot.status = next;
        self.snapshot.updated_at = updated_at;
        self.snapshot.error_code = error_code;
        Ok(self.snapshot())
    }

    pub fn record_progress(
        &mut self,
        transferred_bytes: u64,
        updated_at: u64,
    ) -> Result<TransferSnapshot, TransferProgressError> {
        if self.snapshot.status != TransferStatus::Transferring {
            return Err(TransferProgressError::NotTransferring(self.snapshot.status));
        }
        if transferred_bytes < self.snapshot.transferred_bytes
            || transferred_bytes > self.snapshot.total_bytes
        {
            return Err(TransferProgressError::InvalidByteCount {
                previous: self.snapshot.transferred_bytes,
                next: transferred_bytes,
                total: self.snapshot.total_bytes,
            });
        }
        self.snapshot.transferred_bytes = transferred_bytes;
        self.snapshot.updated_at = updated_at;
        Ok(self.snapshot())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("invalid transfer transition from {current:?} to {next:?}")]
pub struct TransferTransitionError {
    pub current: TransferStatus,
    pub next: TransferStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum TransferProgressError {
    #[error("cannot update progress while transfer is {0:?}")]
    NotTransferring(TransferStatus),
    #[error("invalid transfer byte count {next}; previous is {previous} and total is {total}")]
    InvalidByteCount {
        previous: u64,
        next: u64,
        total: u64,
    },
}

/// Immutable view of synchronized text clipboard state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardSnapshot {
    pub text: String,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_device_id: Option<String>,
}

/// Transport-independent mutations accepted by the application core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    AnnounceDiscovery,
    ForgetDevice { device_id: String },
    StartPairing { device_id: String },
    AcceptPairing { pairing_id: Uuid },
    CancelPairing { pairing_id: Uuid },
    StartTransfer { device_id: String, file: PathBuf },
    CancelTransfer { transfer_id: Uuid },
    SetClipboard { text: String },
}

/// Transport-independent reads accepted by the application core.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    Status,
    Devices,
    Device { device_id: String },
    Pairing { pairing_id: Uuid },
    Transfers,
    Transfer { transfer_id: Uuid },
    Clipboard,
}

/// Typed result of an application query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryResult {
    Status(StatusSnapshot),
    Devices(Vec<DeviceSnapshot>),
    Device(Option<DeviceSnapshot>),
    Pairing(Option<PairingSnapshot>),
    Transfers(Vec<TransferSnapshot>),
    Transfer(Option<TransferSnapshot>),
    Clipboard(ClipboardSnapshot),
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn pairing(status: PairingStatus) -> Pairing {
        Pairing::new(PairingSnapshot {
            id: Uuid::nil(),
            device_id: "740bd4b9b4184ee497d6caf1da8151be".into(),
            device_name: "FOSS Phone".into(),
            direction: PairingDirection::Outgoing,
            status,
            verification_code: Some("ABCDEF12".into()),
            created_at: 100,
            expires_at: 130,
            error_code: None,
        })
    }

    fn transfer(status: TransferStatus) -> Transfer {
        Transfer::new(TransferSnapshot {
            id: Uuid::nil(),
            device_id: "740bd4b9b4184ee497d6caf1da8151be".into(),
            device_name: "FOSS Phone".into(),
            direction: TransferDirection::Outgoing,
            status,
            file_name: "photo.jpg".into(),
            total_bytes: 10,
            transferred_bytes: 0,
            created_at: 100,
            updated_at: 100,
            error_code: None,
        })
    }

    #[test]
    fn pairing_rejects_invalid_transitions() {
        let mut pairing = pairing(PairingStatus::Requested);
        let error = pairing
            .transition(PairingStatus::Accepted, None)
            .unwrap_err();
        assert_eq!(error.current, PairingStatus::Requested);
        assert_eq!(error.next, PairingStatus::Accepted);
        assert_eq!(pairing.snapshot().status, PairingStatus::Requested);

        pairing
            .transition(PairingStatus::AwaitingConfirmation, None)
            .unwrap();
        pairing.transition(PairingStatus::Accepted, None).unwrap();
        assert!(pairing.transition(PairingStatus::Rejected, None).is_err());
    }

    #[test]
    fn transfer_rejects_invalid_transitions_and_progress() {
        let mut transfer = transfer(TransferStatus::Queued);
        assert!(
            transfer
                .transition(TransferStatus::Completed, 101, None)
                .is_err()
        );
        transfer
            .transition(TransferStatus::Transferring, 102, None)
            .unwrap();
        transfer.record_progress(8, 103).unwrap();
        assert!(matches!(
            transfer.record_progress(7, 104),
            Err(TransferProgressError::InvalidByteCount { .. })
        ));
        assert!(transfer.record_progress(11, 104).is_err());
    }

    #[test]
    fn snapshots_use_stable_camel_case_json_names() {
        let status = StatusSnapshot {
            version: "0.1.0".into(),
            uptime_seconds: 7,
            local_device: LocalDeviceSnapshot {
                device_id: "local".into(),
                device_name: "Desk".into(),
            },
            protocol_version: 8,
        };
        assert_eq!(
            serde_json::to_value(status).unwrap(),
            json!({
                "version": "0.1.0",
                "uptimeSeconds": 7,
                "localDevice": {"deviceId": "local", "deviceName": "Desk"},
                "protocolVersion": 8
            })
        );

        let pairing_json =
            serde_json::to_value(pairing(PairingStatus::AwaitingConfirmation).snapshot()).unwrap();
        assert_eq!(pairing_json["verificationCode"], "ABCDEF12");
        assert_eq!(pairing_json["createdAt"], 100);
        assert_eq!(pairing_json["expiresAt"], 130);

        let transfer_json =
            serde_json::to_value(transfer(TransferStatus::Queued).snapshot()).unwrap();
        assert_eq!(transfer_json["fileName"], "photo.jpg");
        assert_eq!(transfer_json["totalBytes"], 10);

        let clipboard = ClipboardSnapshot {
            text: "hello".into(),
            updated_at: 101,
            source_device_id: Some("peer".into()),
        };
        assert_eq!(
            serde_json::to_value(clipboard).unwrap(),
            json!({"text": "hello", "updatedAt": 101, "sourceDeviceId": "peer"})
        );
    }
}
