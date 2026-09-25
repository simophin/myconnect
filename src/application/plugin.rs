//! The seam between the core and the features built on it.
//!
//! A feature implements [`Plugin`]: it names the packet types it receives
//! and sends, handles packets from paired devices, and brings its own HTTP
//! routes. The core owns connections, pairing and the event bus, and gives
//! plugins a [`PluginContext`] to reach them. The set of plugins is fixed at
//! compile time ([`crate::plugins::builtin`]); nothing is loaded at runtime.
//!
//! Features not yet moved to a plugin are still routed by the fixed table in
//! [`crate::plugins`]. See `docs/research/feature-modules.md`.

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use axum::Router;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use super::{ApplicationError, ApplicationHandle, EventData};
use crate::{device::DeviceSnapshot, protocol::Packet};

/// A feature of the daemon, plugged into the core.
pub trait Plugin: Send + Sync + 'static {
    /// Stable identifier, e.g. `"ping"`.
    fn id(&self) -> &'static str;

    /// Packet types this plugin handles; advertised as incoming
    /// capabilities. None by default, for a plugin that only sends.
    fn incoming(&self) -> &'static [&'static str] {
        &[]
    }

    /// Packet types this plugin sends; advertised as outgoing capabilities.
    fn outgoing(&self) -> &'static [&'static str];

    /// Handle a packet of one of [`Self::incoming`]'s types. The core calls
    /// this only for devices that are paired, and never while holding its
    /// own state lock. A plugin that declares incoming types must override
    /// it.
    fn handle_packet(&self, _ctx: &PluginContext, _device: &DeviceSnapshot, _packet: &Packet) {}

    /// HTTP routes under `/api/v1`, with their state already applied. They
    /// get the standard body limit, request deadline and authentication.
    fn routes(self: Arc<Self>, _ctx: PluginContext) -> Router {
        Router::new()
    }

    /// What this plugin adds to a device's snapshot, under its
    /// [`Self::id`] in `plugins`; `None` to add nothing. The core asks each
    /// time it hands out a snapshot, never while holding its own lock. A
    /// plugin whose answer changes calls [`PluginContext::device_changed`].
    fn device_state(&self, _device_id: &str) -> Option<Value> {
        None
    }

    /// The device's connection closed, or the device was forgotten while
    /// connected. Called before the core publishes the device's new state,
    /// so state cleared here needs no [`PluginContext::device_changed`].
    fn disconnected(&self, _ctx: &PluginContext, _device_id: &str) {}

    /// The device is no longer paired: it unpaired us, or it was forgotten.
    /// Called before the core publishes the device's new state, as for
    /// [`Self::disconnected`].
    fn unpaired(&self, _ctx: &PluginContext, _device_id: &str) {}
}

/// What the core offers a plugin. Cheap to clone.
#[derive(Clone)]
pub struct PluginContext {
    core: ApplicationHandle,
}

impl PluginContext {
    pub(super) fn new(core: ApplicationHandle) -> Self {
        Self { core }
    }

    /// Queue `packet` to a device, provided it is paired, connected, and has
    /// advertised the packet's type in its incoming capabilities.
    pub fn send(&self, device_id: &str, packet: Packet) -> Result<(), ApplicationError> {
        self.core.send_to_capable(device_id, packet)
    }

    /// Tell clients that what a plugin adds to a device's snapshot
    /// ([`Plugin::device_state`]) changed: publishes `device.updated`.
    pub fn device_changed(&self, device_id: &str) {
        self.core.publish_device_update(device_id);
    }

    /// Publish a plugin event to `/events` subscribers.
    pub fn publish<T: PluginEventKind>(&self, event: &T) -> Result<(), ApplicationError> {
        let event = PluginEvent::new(event).map_err(|_| ApplicationError::Internal)?;
        self.core.event_bus().publish(EventData::Plugin(event))?;
        Ok(())
    }
}

/// An event type owned by a plugin. `TYPE` is its name on the wire, e.g.
/// `"ping.received"`.
pub trait PluginEventKind: Serialize + DeserializeOwned {
    const TYPE: &'static str;
}

/// A plugin's event as carried by the event bus: its type name and JSON
/// data, serialized the same way as core events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginEvent {
    #[serde(rename = "type")]
    event_type: String,
    data: Value,
}

impl PluginEvent {
    pub fn new<T: PluginEventKind>(event: &T) -> Result<Self, serde_json::Error> {
        Ok(Self {
            event_type: T::TYPE.to_owned(),
            data: serde_json::to_value(event)?,
        })
    }

    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// The event as `T`, if it is one.
    pub fn decode<T: PluginEventKind>(&self) -> Option<T> {
        if self.event_type != T::TYPE {
            return None;
        }
        serde_json::from_value(self.data.clone()).ok()
    }
}

/// The plugins of this build, indexed by the packet types they handle.
pub struct PluginRegistry {
    plugins: Vec<Arc<dyn Plugin>>,
    by_packet_type: HashMap<&'static str, usize>,
}

impl PluginRegistry {
    /// # Panics
    ///
    /// If two plugins share an id or claim the same incoming packet type: a
    /// build error that every test would hit.
    pub fn new(plugins: Vec<Arc<dyn Plugin>>) -> Self {
        let mut by_packet_type = HashMap::new();
        for (index, plugin) in plugins.iter().enumerate() {
            if plugins[..index]
                .iter()
                .any(|other| other.id() == plugin.id())
            {
                panic!("two plugins are called {:?}", plugin.id());
            }
            for packet_type in plugin.incoming() {
                if let Some(other) = by_packet_type.insert(*packet_type, index) {
                    panic!(
                        "plugins {:?} and {:?} both handle {packet_type:?}",
                        plugins[other].id(),
                        plugin.id()
                    );
                }
            }
        }
        Self {
            plugins,
            by_packet_type,
        }
    }

    /// The plugin that handles `packet_type`, if any.
    pub fn for_packet(&self, packet_type: &str) -> Option<&Arc<dyn Plugin>> {
        self.by_packet_type
            .get(packet_type)
            .map(|index| &self.plugins[*index])
    }

    pub fn incoming(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.incoming())
            .copied()
    }

    pub fn outgoing(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.plugins
            .iter()
            .flat_map(|plugin| plugin.outgoing())
            .copied()
    }

    /// What every plugin adds to a device's snapshot, keyed by plugin id.
    pub fn device_state(&self, device_id: &str) -> BTreeMap<String, Value> {
        self.plugins
            .iter()
            .filter_map(|plugin| {
                let state = plugin.device_state(device_id)?;
                Some((plugin.id().to_owned(), state))
            })
            .collect()
    }

    pub fn disconnected(&self, ctx: &PluginContext, device_id: &str) {
        for plugin in &self.plugins {
            plugin.disconnected(ctx, device_id);
        }
    }

    pub fn unpaired(&self, ctx: &PluginContext, device_id: &str) {
        for plugin in &self.plugins {
            plugin.unpaired(ctx, device_id);
        }
    }

    /// Every plugin's routes, merged.
    pub fn routes(&self, ctx: &PluginContext) -> Router {
        self.plugins.iter().fold(Router::new(), |router, plugin| {
            router.merge(plugin.clone().routes(ctx.clone()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Waved {
        hand: String,
    }

    impl PluginEventKind for Waved {
        const TYPE: &'static str = "wave.received";
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Other {}

    impl PluginEventKind for Other {
        const TYPE: &'static str = "other.happened";
    }

    #[test]
    fn plugin_events_decode_only_as_their_own_type() {
        let event = PluginEvent::new(&Waved {
            hand: "left".into(),
        })
        .unwrap();
        assert_eq!(event.event_type(), "wave.received");
        assert_eq!(
            event.decode::<Waved>(),
            Some(Waved {
                hand: "left".into()
            })
        );
        assert_eq!(event.decode::<Other>(), None);
    }

    struct Claims(&'static str, &'static [&'static str]);

    impl Plugin for Claims {
        fn id(&self) -> &'static str {
            self.0
        }
        fn incoming(&self) -> &'static [&'static str] {
            self.1
        }
        fn outgoing(&self) -> &'static [&'static str] {
            &[]
        }
        fn handle_packet(&self, _: &PluginContext, _: &DeviceSnapshot, _: &Packet) {}
    }

    #[test]
    fn packets_route_to_the_plugin_that_claims_them() {
        let registry = PluginRegistry::new(vec![
            Arc::new(Claims("a", &["x.one"])),
            Arc::new(Claims("b", &["x.two", "x.three"])),
        ]);
        assert_eq!(registry.for_packet("x.three").unwrap().id(), "b");
        assert!(registry.for_packet("x.four").is_none());
        assert_eq!(
            registry.incoming().collect::<Vec<_>>(),
            ["x.one", "x.two", "x.three"]
        );
    }

    /// A plugin that only sends `x.wave`, as a feature would through its
    /// context.
    struct Waver;

    impl Waver {
        const PACKET_TYPE: &'static str = "x.wave";

        fn wave(ctx: &PluginContext, device_id: &str) -> Result<(), ApplicationError> {
            ctx.send(
                device_id,
                Packet::from_body(1_u64, Self::PACKET_TYPE, &serde_json::json!({})).unwrap(),
            )
        }
    }

    impl Plugin for Waver {
        fn id(&self) -> &'static str {
            "wave"
        }
        fn outgoing(&self) -> &'static [&'static str] {
            &[Self::PACKET_TYPE]
        }
    }

    #[test]
    fn plugins_send_only_to_paired_connected_devices_that_accept_the_packet_type() {
        use tokio::sync::mpsc;
        use tokio_util::sync::CancellationToken;

        use crate::application::testing::{handle, make_identity};

        let (handle, _commands) = handle();
        let ctx = handle.plugin_context();
        let device_id = "740bd4b9b4184ee497d6caf1da8151be";
        assert_eq!(Waver.outgoing(), [Waver::PACKET_TYPE]);
        assert!(matches!(
            Waver::wave(&ctx, device_id),
            Err(ApplicationError::UnknownDevice)
        ));

        let accepting = make_identity(device_id, vec![Waver::PACKET_TYPE.into()]);
        handle.discover_device(&accepting, false, 1).unwrap();
        assert!(matches!(
            Waver::wave(&ctx, device_id),
            Err(ApplicationError::NotPaired)
        ));

        handle.discover_device(&accepting, true, 2).unwrap();
        assert!(matches!(
            Waver::wave(&ctx, device_id),
            Err(ApplicationError::DeviceNotConnected)
        ));

        // Paired and connected, but the peer never advertised the packet
        // type: refused with a typed error, not dropped silently.
        let other = make_identity(device_id, vec!["x.other".into()]);
        handle.discover_device(&other, true, 3).unwrap();
        let (tx, mut rx) = mpsc::channel(4);
        handle
            .register_connection(device_id, vec![1, 2, 3], 8, tx, CancellationToken::new(), 3)
            .unwrap();
        assert!(matches!(
            Waver::wave(&ctx, device_id),
            Err(ApplicationError::UnsupportedByPeer)
        ));
        assert!(rx.try_recv().is_err());

        // It re-announces (e.g. on reconnect) accepting it.
        handle.discover_device(&accepting, true, 4).unwrap();
        Waver::wave(&ctx, device_id).unwrap();
        assert_eq!(rx.try_recv().unwrap().packet_type, Waver::PACKET_TYPE);
    }

    #[test]
    #[should_panic(expected = "two plugins are called \"a\"")]
    fn two_plugins_cannot_share_an_id() {
        PluginRegistry::new(vec![
            Arc::new(Claims("a", &["x.one"])),
            Arc::new(Claims("a", &["x.two"])),
        ]);
    }

    #[test]
    #[should_panic(expected = "both handle \"x.one\"")]
    fn two_plugins_cannot_claim_one_packet_type() {
        PluginRegistry::new(vec![
            Arc::new(Claims("a", &["x.one"])),
            Arc::new(Claims("b", &["x.one"])),
        ]);
    }
}
