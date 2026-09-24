//! `kdeconnect.battery` packet model.
//!
//! A peer reports its battery whenever the level or charging state changes,
//! and once when the plugin loads, which is right after pairing or
//! connecting. Listing the packet type among our incoming capabilities is
//! what makes KDE Connect for Android send it; nothing needs to be
//! requested. This build doesn't report a battery of its own.

use serde::{Deserialize, Serialize};

use crate::device::BatteryStatus;

/// The packet type and capability identifier for battery reports.
pub const PACKET_TYPE: &str = "kdeconnect.battery";

/// Body of a `kdeconnect.battery` packet. KDE Connect desktops send a
/// `currentCharge` of `-1` when they have no battery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryBody {
    #[serde(default = "no_charge")]
    pub current_charge: i64,
    #[serde(default)]
    pub is_charging: bool,
    /// `1` when the report was sent because the battery ran low, else `0`.
    #[serde(default)]
    pub threshold_event: i64,
}

fn no_charge() -> i64 {
    -1
}

impl BatteryBody {
    /// The battery this report describes, or `None` if the peer has none.
    pub fn status(&self) -> Option<BatteryStatus> {
        let charge = u8::try_from(self.current_charge).ok()?;
        Some(BatteryStatus {
            charge: charge.min(100),
            charging: self.is_charging,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::Packet;

    fn body(value: serde_json::Value) -> BatteryBody {
        Packet::from_body(1_u64, PACKET_TYPE, &value)
            .unwrap()
            .body_as()
            .unwrap()
    }

    #[test]
    fn reads_an_android_report() {
        let report = body(json!({"currentCharge": 82, "isCharging": true, "thresholdEvent": 0}));
        assert_eq!(
            report.status(),
            Some(BatteryStatus {
                charge: 82,
                charging: true
            })
        );
    }

    #[test]
    fn a_negative_or_missing_charge_means_no_battery() {
        assert_eq!(body(json!({"currentCharge": -1})).status(), None);
        assert_eq!(body(json!({})).status(), None);
    }

    #[test]
    fn an_out_of_range_charge_is_capped() {
        assert_eq!(
            body(json!({"currentCharge": 250})).status(),
            Some(BatteryStatus {
                charge: 100,
                charging: false
            })
        );
    }
}
