//! The core of the daemon: devices, connections, pairing, transfers,
//! settings and events, and the plugin API that features are built on.
//! It knows no feature by name; [`crate::daemon`] decides which plugins
//! run.

mod events;
mod payload;
mod plugin;
mod service;
mod settings;
mod state;
#[cfg(test)]
pub(crate) mod testing;
mod transfers;

pub(crate) use settings::Settings;

pub use events::{CoreEvent, EventBus, EventBusError, EventData};
pub use payload::{AcceptedPayload, DialedPayload, PayloadListener, PayloadPeer, SshAuthError};
pub use plugin::{
    Capabilities, Plugin, PluginContext, PluginEvent, PluginEventKind, PluginRegistry,
    PluginSettings, SettingsSection,
};
pub use service::{Core, CoreError};
pub use settings::{SettingsDefaults, SettingsPatch, SettingsSnapshot};
pub use state::{
    LanCommand, LocalDeviceSnapshot, OperationErrorCode, Pairing, PairingDirection,
    PairingSnapshot, PairingStatus, PairingTransitionError, StatusSnapshot, Transfer,
    TransferDirection, TransferProgressError, TransferSnapshot, TransferStatus,
    TransferTransitionError,
};
pub use transfers::{
    DEFAULT_MAX_TRANSFER_BYTES, FileNameError, PROGRESS_EVENT_INTERVAL, TransferConfig,
    TransferHandle, Transfers, sanitize_file_name, upload_channel,
};
