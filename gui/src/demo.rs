//! `--demo`: made-up paired devices, fed through the core's own entry
//! points so the UI sees them as ordinary snapshots and events. Nothing is
//! written to the trust store; they vanish on exit.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use myconnect::{
    core::Core,
    protocol::{DeviceType, IdentityBody, Packet},
};
use serde_json::json;

const PHONE: &str = "demo0phone00000000000000000000001";
const TABLET: &str = "demo0tablet0000000000000000000002";
const LAPTOP: &str = "demo0laptop0000000000000000000003";
const TV: &str = "demo0tv000000000000000000000000004";

/// Add the demo devices, then keep changing them so live updates show:
/// the TV comes and goes, and the phone's battery drains.
pub async fn run(core: Core) {
    let devices = [
        (PHONE, "Pixel 8a", DeviceType::Phone, true),
        (TABLET, "Galaxy Tab S9", DeviceType::Tablet, true),
        (LAPTOP, "Work laptop", DeviceType::Laptop, false),
        (TV, "Living room TV", DeviceType::Tv, false),
    ];
    for (id, name, device_type, connected) in devices {
        let _ = core.discover_device(&identity(id, name, device_type), true, now());
        if connected {
            let _ = core.mark_device_connected(id, now());
        } else {
            let _ = core.mark_device_disconnected(id);
        }
    }
    battery(&core, TABLET, 45, true);

    let mut charge = 82;
    let mut tv_connected = false;
    loop {
        battery(&core, PHONE, charge, false);
        tokio::time::sleep(Duration::from_secs(3)).await;
        charge = if charge <= 5 { 100 } else { charge - 7 };
        tv_connected = !tv_connected;
        let _ = if tv_connected {
            core.mark_device_connected(TV, now())
        } else {
            core.mark_device_disconnected(TV)
        };
    }
}

fn battery(core: &Core, device_id: &str, charge: i64, charging: bool) {
    let body = json!({"currentCharge": charge, "isCharging": charging, "thresholdEvent": 0});
    if let Ok(packet) = Packet::from_body(0, "kdeconnect.battery", &body) {
        core.handle_peer_packet(device_id, packet);
    }
}

fn identity(device_id: &str, name: &str, device_type: DeviceType) -> IdentityBody {
    IdentityBody {
        device_id: device_id.into(),
        device_name: name.into(),
        device_type,
        incoming_capabilities: vec!["kdeconnect.battery".into()],
        outgoing_capabilities: vec!["kdeconnect.battery".into()],
        protocol_version: 8,
        extra: Default::default(),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}
