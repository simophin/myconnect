//! The seam between the core and the features built on it.
//!
//! A feature implements [`Plugin`]: it names the packet types it receives
//! and sends, handles packets from paired devices, and brings its own HTTP
//! routes. The core owns connections, pairing and the event bus, and gives
//! plugins a [`PluginContext`] to reach them. The set of plugins is fixed at
//! compile time ([`crate::plugins::builtin`]); nothing is loaded at runtime.
//!
//! See `docs/research/feature-modules.md` for how the daemon got this
//! shape.

use std::{
    any::Any,
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use axum::Router;
use futures_util::future::{BoxFuture, join_all};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};

use super::{ApplicationError, ApplicationHandle, EventData, PayloadPeer, Transfers};
use crate::{device::DeviceSnapshot, protocol::Packet};

/// A feature of the daemon, plugged into the core.
pub trait Plugin: Any + Send + Sync {
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

    /// HTTP routes under `/api/v1` that take a large, streamed request body
    /// (an upload), with their state already applied. They get the
    /// transfer-sized body limit instead of the standard one and no overall
    /// deadline; a handler bounds each step of the upload with the
    /// [`crate::api::UploadIdleTimeout`] in its request's extensions.
    /// Authentication applies as for [`Self::routes`].
    fn streaming_routes(self: Arc<Self>, _ctx: PluginContext) -> Router {
        Router::new()
    }

    /// What this plugin adds to a device's snapshot, under its
    /// [`Self::id`] in `plugins`; `None` to add nothing. The core asks each
    /// time it hands out a snapshot, never while holding its own lock. A
    /// plugin whose answer changes calls [`PluginContext::device_changed`].
    fn device_state(&self, _device_id: &str) -> Option<Value> {
        None
    }

    /// This plugin's section of the settings, if it has one: see
    /// [`PluginSettings`].
    fn settings(&self) -> Option<SettingsSection> {
        None
    }

    /// A connection to the device was registered. Called after the core
    /// publishes `device.connected`, never while holding its own lock. The
    /// device may not be paired; [`PluginContext::send`] checks that.
    fn connected(&self, _ctx: &PluginContext, _device: &DeviceSnapshot) {}

    /// The device's connection closed, or the device was forgotten while
    /// connected. Called before the core publishes the device's new state,
    /// so state cleared here needs no [`PluginContext::device_changed`].
    fn disconnected(&self, _ctx: &PluginContext, _device_id: &str) {}

    /// The device is no longer paired: it unpaired us, or it was forgotten.
    /// Called before the core publishes the device's new state, as for
    /// [`Self::disconnected`].
    fn unpaired(&self, _ctx: &PluginContext, _device_id: &str) {}

    /// The daemon started: called once, inside the async runtime, before
    /// the LAN transport and the API start, for a plugin that runs work of
    /// its own (e.g. watching something on this machine). A core built
    /// without the daemon, as in unit tests, never calls it.
    fn started(self: Arc<Self>, _ctx: &PluginContext) {}

    /// The daemon is stopping: end the plugin's own work and close what it
    /// holds open. Called once, after the API and LAN transport have
    /// stopped and every transfer has ended, never while holding the
    /// core's lock. Every plugin's shutdown runs concurrently.
    fn shutdown(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
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

    /// Whether [`Self::send`] would take a packet of `packet_type` for the
    /// device now, and the error it would refuse it with if not; for a
    /// plugin that has work to do before it can build the packet.
    pub fn can_send(&self, device_id: &str, packet_type: &str) -> Result<(), ApplicationError> {
        self.core.check_capable(device_id, packet_type)
    }

    /// Queue `packet` to every paired, connected device that has advertised
    /// its type, except `except` (e.g. the device it came from).
    pub fn broadcast(&self, packet: &Packet, except: Option<&str>) {
        self.core.broadcast_to_capable(packet, except);
    }

    /// The plugin's settings section, as in effect now: stored values over
    /// the section's defaults.
    pub fn settings<T: PluginSettings>(&self) -> T {
        self.core
            .plugin_settings(T::ID)
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    /// The device as clients see it, if it is known. Calls into every
    /// plugin's [`Plugin::device_state`], so don't hold a lock of your own
    /// while calling it.
    pub fn device(&self, device_id: &str) -> Option<DeviceSnapshot> {
        self.core.device(device_id)
    }

    /// The transfers service: every feature that moves a file records it
    /// there, so it is listed, reports progress and can be cancelled.
    pub fn transfers(&self) -> &Transfers {
        self.core.transfers()
    }

    /// What it takes to open payload connections with a paired, connected
    /// device, for moving a file's bytes beside the control connection.
    pub fn payload_peer(&self, device_id: &str) -> Result<PayloadPeer, ApplicationError> {
        self.core.payload_peer(device_id)
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

/// A plugin's settings: a section of `settings.json` and of
/// `GET`/`PATCH /settings`, under `plugins.<ID>`.
///
/// Every field has a default (`#[serde(default)]` on the type), so a section
/// stores only what the user changed. A `PATCH` merges fields into the
/// stored section, `null` resetting one to its default; the result must
/// deserialize as `Self` to be accepted, so `#[serde(deny_unknown_fields)]`
/// is how a plugin rejects unknown fields. The plugin reads the settings in
/// effect with [`PluginContext::settings`].
pub trait PluginSettings: Serialize + DeserializeOwned + Default {
    /// The plugin's [`Plugin::id`].
    const ID: &'static str;
}

/// A settings section, as the core handles it: [`PluginSettings`] with the
/// type erased.
#[derive(Clone, Copy, Debug)]
pub struct SettingsSection {
    pub(super) id: &'static str,
    resolve: fn(&Map<String, Value>) -> Option<Value>,
}

impl SettingsSection {
    pub fn of<T: PluginSettings>() -> Self {
        Self {
            id: T::ID,
            resolve: |stored| {
                let settings: T = serde_json::from_value(Value::Object(stored.clone())).ok()?;
                serde_json::to_value(settings).ok()
            },
        }
    }

    /// The section in effect given what is stored: every field, defaults
    /// filled in. `None` if `stored` isn't valid.
    pub(super) fn resolve(&self, stored: &Map<String, Value>) -> Option<Value> {
        (self.resolve)(stored)
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
            if let Some(section) = plugin.settings() {
                assert_eq!(
                    section.id,
                    plugin.id(),
                    "a plugin's settings section is named after it"
                );
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

    /// The plugin of type `T`, if this build has one.
    pub fn get<T: Plugin>(&self) -> Option<Arc<T>> {
        self.plugins.iter().find_map(|plugin| {
            let plugin: Arc<dyn Any + Send + Sync> = plugin.clone();
            plugin.downcast::<T>().ok()
        })
    }

    /// Every plugin's settings section.
    pub fn settings_sections(&self) -> Vec<SettingsSection> {
        self.plugins
            .iter()
            .filter_map(|plugin| plugin.settings())
            .collect()
    }

    pub fn connected(&self, ctx: &PluginContext, device: &DeviceSnapshot) {
        for plugin in &self.plugins {
            plugin.connected(ctx, device);
        }
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

    pub fn started(&self, ctx: &PluginContext) {
        for plugin in &self.plugins {
            plugin.clone().started(ctx);
        }
    }

    pub async fn shutdown(&self) {
        join_all(self.plugins.iter().map(|plugin| plugin.shutdown())).await;
    }

    /// Every plugin's routes, merged.
    pub fn routes(&self, ctx: &PluginContext) -> Router {
        self.plugins.iter().fold(Router::new(), |router, plugin| {
            router.merge(plugin.clone().routes(ctx.clone()))
        })
    }

    /// Every plugin's streaming routes, merged.
    pub fn streaming_routes(&self, ctx: &PluginContext) -> Router {
        self.plugins.iter().fold(Router::new(), |router, plugin| {
            router.merge(plugin.clone().streaming_routes(ctx.clone()))
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
    fn broadcasts_reach_every_paired_connected_device_that_accepts_them_but_one() {
        use tokio::sync::mpsc;
        use tokio_util::sync::CancellationToken;

        use crate::application::testing::{handle, make_identity};

        let (handle, _commands) = handle();
        let connect = |device_id: &str, paired: bool, accepts: bool| {
            let capabilities = if accepts {
                vec![Waver::PACKET_TYPE.into()]
            } else {
                Vec::new()
            };
            handle
                .discover_device(&make_identity(device_id, capabilities), paired, 1)
                .unwrap();
            let (tx, rx) = mpsc::channel(4);
            handle
                .register_connection(device_id, vec![1], 8, tx, CancellationToken::new(), 1)
                .unwrap();
            rx
        };
        let mut source = connect("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", true, true);
        let mut other = connect("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", true, true);
        let mut unpaired = connect("cccccccccccccccccccccccccccccccc", false, true);
        let mut refusing = connect("dddddddddddddddddddddddddddddddd", true, false);

        let packet = Packet::from_body(1_u64, Waver::PACKET_TYPE, &serde_json::json!({})).unwrap();
        handle
            .plugin_context()
            .broadcast(&packet, Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert_eq!(other.try_recv().unwrap(), packet);
        assert!(source.try_recv().is_err());
        assert!(unpaired.try_recv().is_err());
        assert!(refusing.try_recv().is_err());
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
