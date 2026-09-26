//! Battery's UI: a device's battery as its status chip. It has no
//! messages: it only shows what the device reports.

use iced_fonts::lucide;
use serde_json::json;

use super::DeviceStatus;
use crate::{
    core::DeviceSnapshot,
    plugins::battery::{BatteryStatus, PACKET_TYPE},
    protocol::{DeviceType, Packet},
    ui::widgets::Icon,
};

pub fn device_status(device: &DeviceSnapshot) -> Option<DeviceStatus> {
    let battery = BatteryStatus::of(device)?;
    Some(DeviceStatus {
        icon: Level::of(battery).icon(),
        label: format!("{}%", battery.charge),
    })
}

/// `--demo`: the phone drains 7% a step and recharges when nearly flat;
/// the tablet sits on its charger.
pub fn demo_packets(device: &DeviceSnapshot, tick: u64) -> Vec<Packet> {
    let (charge, charging) = match device.device_type {
        DeviceType::Phone => (100 - (18 + 7 * tick) % 96, false),
        DeviceType::Tablet => (45, true),
        _ => return Vec::new(),
    };
    let body = json!({"currentCharge": charge, "isCharging": charging, "thresholdEvent": 0});
    Packet::from_body(0, PACKET_TYPE, &body)
        .into_iter()
        .collect()
}

/// Which icon a battery shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Charging,
    Low,
    Medium,
    Full,
}

impl Level {
    fn of(battery: BatteryStatus) -> Self {
        if battery.charging {
            Self::Charging
        } else if battery.charge <= 15 {
            Self::Low
        } else if battery.charge <= 60 {
            Self::Medium
        } else {
            Self::Full
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Charging => lucide::battery_charging,
            Self::Low => lucide::battery_low,
            Self::Medium => lucide::battery_medium,
            Self::Full => lucide::battery_full,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::DeviceReachability,
        ui::{pages::devices, testing},
    };

    fn device(name: &str, battery: Option<BatteryStatus>) -> DeviceSnapshot {
        let mut device = testing::device(name);
        if let Some(battery) = battery {
            device.plugins.insert(
                crate::plugins::battery::ID.into(),
                serde_json::to_value(battery).unwrap(),
            );
        }
        device
    }

    fn level(charge: u8, charging: bool) -> Level {
        Level::of(BatteryStatus { charge, charging })
    }

    #[test]
    fn shows_the_charge_with_an_icon_by_level() {
        assert_eq!(level(10, true), Level::Charging);
        assert_eq!(level(15, false), Level::Low);
        assert_eq!(level(16, false), Level::Medium);
        assert_eq!(level(60, false), Level::Medium);
        assert_eq!(level(61, false), Level::Full);
        let battery = BatteryStatus {
            charge: 82,
            charging: false,
        };
        let status = device_status(&device("Phone", Some(battery))).expect("a status");
        assert_eq!(status.label, "82%");
    }

    #[test]
    fn shows_nothing_until_the_device_reports() {
        assert!(device_status(&device("Phone", None)).is_none());
    }

    #[test]
    fn the_demo_phone_drains_and_recharges() {
        let phone = device("Phone", None);
        let charges: Vec<i64> = (0..16)
            .map(|tick| {
                let [packet] = demo_packets(&phone, tick).try_into().unwrap();
                packet
                    .body_as::<crate::plugins::battery::BatteryBody>()
                    .unwrap()
                    .current_charge
            })
            .collect();
        assert_eq!(charges[0], 82);
        assert_eq!(charges[1], 75);
        assert!(charges.iter().all(|charge| (1..=100).contains(charge)));
        assert!(
            charges.windows(2).any(|pair| pair[1] > pair[0]),
            "recharges"
        );
    }

    #[test]
    fn snapshot_devices_with_battery() {
        let mut tablet = device(
            "Galaxy Tab S9",
            Some(BatteryStatus {
                charge: 45,
                charging: true,
            }),
        );
        tablet.device_type = DeviceType::Tablet;
        let mut laptop = device("Work laptop", None);
        laptop.device_type = DeviceType::Laptop;
        laptop.reachability = DeviceReachability::Unavailable;
        let store = testing::store(
            "Demo desktop",
            vec![
                device(
                    "Pixel 8a",
                    Some(BatteryStatus {
                        charge: 12,
                        charging: false,
                    }),
                ),
                tablet,
                laptop,
            ],
        );
        let statuses = |device: &DeviceSnapshot| -> Vec<DeviceStatus> {
            device_status(device).into_iter().collect()
        };
        testing::snapshot("devices-battery", (440.0, 400.0), || {
            devices::view(&store, &statuses, |_| (), ())
        });
    }
}
