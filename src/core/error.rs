//! The core's errors, and the failure codes it reports to clients.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{EventBusError, PairingTransitionError};
use crate::config::{SettingsError, TrustError};

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
    #[error("a transfer with this id already exists")]
    TransferExists,
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
