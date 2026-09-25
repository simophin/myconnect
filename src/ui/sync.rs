//! The one subscription that watches the in-process core: a snapshot, then
//! the core's events, and a fresh snapshot whenever the receiver lags.

use std::hash::{Hash, Hasher};

use iced::{
    Subscription,
    futures::{SinkExt, Stream},
};
use tokio::sync::broadcast::error::RecvError;

use crate::core::{Core, CoreEvent, DeviceSnapshot};

/// What the core reports to the UI.
#[derive(Debug, Clone)]
pub enum Update {
    /// The whole state, sent first and again after a lag.
    Snapshot {
        devices: Vec<DeviceSnapshot>,
        local_name: String,
    },
    /// One change since the last snapshot.
    Event(Box<CoreEvent>),
}

/// Watch `core`. Subscribing before taking the snapshot means no change is
/// missed; events already reflected in the snapshot are replayed after it.
pub fn watch(core: &Core) -> Subscription<Update> {
    Subscription::run_with(Watched(core.clone()), stream)
}

/// The core, as the key of the subscription that watches it.
struct Watched(Core);

impl Hash for Watched {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.local_device_id().hash(state);
    }
}

fn stream(watched: &Watched) -> impl Stream<Item = Update> + use<> {
    let core = watched.0.clone();
    iced::stream::channel(64, async move |mut output| {
        loop {
            let mut events = core.subscribe();
            let snapshot = Update::Snapshot {
                devices: core.devices().unwrap_or_default(),
                local_name: core.local_device_name(),
            };
            if output.send(snapshot).await.is_err() {
                return;
            }
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if output.send(Update::Event(Box::new(event))).await.is_err() {
                            return;
                        }
                    }
                    Err(RecvError::Lagged(_)) => break,
                    Err(RecvError::Closed) => return,
                }
            }
        }
    })
}
