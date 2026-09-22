use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
    time::Instant,
};

use thiserror::Error;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use super::{
    ApplicationEvent, ClipboardSnapshot, Command, EventBus, EventBusError, LocalDeviceSnapshot,
    PairingSnapshot, Query, QueryResult, StatusSnapshot, TransferSnapshot,
};
use crate::device::DeviceRegistry;

/// Core interface consumed by the local API and future frontends.
pub trait ApplicationService: Send + Sync {
    fn query(&self, query: Query) -> Result<QueryResult, ApplicationError>;
    fn command(&self, command: Command) -> Result<(), ApplicationError>;
    fn subscribe(&self) -> broadcast::Receiver<ApplicationEvent>;
}

struct ApplicationState {
    devices: DeviceRegistry,
    pairings: BTreeMap<Uuid, PairingSnapshot>,
    transfers: BTreeMap<Uuid, TransferSnapshot>,
    clipboard: ClipboardSnapshot,
}

/// Cloneable application facade backed by bounded commands and snapshots.
#[derive(Clone)]
pub struct ApplicationHandle {
    started_at: Instant,
    local_device: LocalDeviceSnapshot,
    protocol_version: u8,
    state: Arc<RwLock<ApplicationState>>,
    commands: mpsc::Sender<Command>,
    events: EventBus,
}

impl ApplicationHandle {
    pub fn new(
        local_device: LocalDeviceSnapshot,
        protocol_version: u8,
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
                state: Arc::new(RwLock::new(ApplicationState {
                    devices: DeviceRegistry::new(),
                    pairings: BTreeMap::new(),
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
            Query::Pairing { pairing_id } => {
                QueryResult::Pairing(state.pairings.get(&pairing_id).cloned())
            }
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle() -> (ApplicationHandle, mpsc::Receiver<Command>) {
        ApplicationHandle::new(
            LocalDeviceSnapshot {
                device_id: "local".into(),
                device_name: "MyConnect".into(),
            },
            8,
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
        assert_eq!(
            handle.command(Command::AnnounceDiscovery),
            Err(ApplicationError::CommandQueueFull)
        );
        assert_eq!(commands.recv().await, Some(Command::AnnounceDiscovery));
    }
}
