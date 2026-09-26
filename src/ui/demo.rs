//! `--demo`: made-up paired devices, fed through the core's own entry
//! points so the UI sees them as ordinary snapshots and events. Nothing is
//! written to the trust store; they vanish on exit.
//!
//! The devices support everything this build does. What they report comes
//! from each UI plugin's [`demo_packets`](super::plugin::UiPlugin::demo_packets).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{
    core::{Core, DeviceReachability},
    protocol::{DeviceType, IdentityBody},
    ui::plugin::ErasedUiPlugin,
};

/// How often [`tick`] runs.
pub const TICK: Duration = Duration::from_secs(3);

const DEVICES: [(&str, &str, DeviceType, bool); 4] = [
    (
        "demo0phone00000000000000000000001",
        "Pixel 8a",
        DeviceType::Phone,
        true,
    ),
    (
        "demo0tablet0000000000000000000002",
        "Galaxy Tab S9",
        DeviceType::Tablet,
        true,
    ),
    (
        "demo0laptop0000000000000000000003",
        "Work laptop",
        DeviceType::Laptop,
        false,
    ),
    (
        "demo0tv000000000000000000000000004",
        "Living room TV",
        DeviceType::Tv,
        false,
    ),
];
const TV: &str = DEVICES[3].0;

/// Add the demo devices.
pub fn start(core: &Core) {
    let capabilities = core.capabilities();
    for (id, name, device_type, connected) in DEVICES {
        // A peer receives what we send, and sends what we receive.
        let identity = IdentityBody {
            device_id: id.into(),
            device_name: name.into(),
            device_type,
            incoming_capabilities: capabilities.outgoing.clone(),
            outgoing_capabilities: capabilities.incoming.clone(),
            protocol_version: 8,
            extra: Default::default(),
        };
        let _ = core.discover_device(&identity, true, now());
        if connected {
            let _ = core.mark_device_connected(id, now());
        } else {
            let _ = core.mark_device_disconnected(id);
        }
    }
}

/// Step `tick` of the demo, every [`TICK`]: the TV comes and goes, and each
/// connected device sends what the plugins make up for it.
pub fn tick(core: &Core, plugins: &[Box<dyn ErasedUiPlugin>], tick: u64) {
    if tick > 0 {
        let _ = if tick % 2 == 1 {
            core.mark_device_connected(TV, now())
        } else {
            core.mark_device_disconnected(TV)
        };
    }
    for (id, ..) in DEVICES {
        let Some(device) = core
            .device(id)
            .filter(|device| device.reachability == DeviceReachability::Connected)
        else {
            continue;
        };
        for packet in plugins
            .iter()
            .flat_map(|plugin| plugin.demo_packets(&device, tick))
        {
            core.handle_peer_packet(id, packet);
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}
