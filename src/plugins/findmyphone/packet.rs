//! `kdeconnect.findmyphone.request` packet model.
//!
//! Asks a peer to ring so it can be found: KDE Connect for Android plays a
//! ringtone loudly until someone dismisses it on the phone. The body is
//! empty. This build only sends the request; it doesn't ring when asked.

use serde_json::{Map, Number};

use crate::protocol::Packet;

/// The packet type and capability identifier for a request to ring.
pub const REQUEST_PACKET_TYPE: &str = "kdeconnect.findmyphone.request";

/// Build a `kdeconnect.findmyphone.request` packet.
pub fn build_request_packet(id: impl Into<Number>) -> Packet {
    Packet {
        id: id.into(),
        packet_type: REQUEST_PACKET_TYPE.to_owned(),
        body: Map::new(),
        payload_size: None,
        payload_transfer_info: None,
        extra: Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_has_an_empty_body() {
        let packet = build_request_packet(1_u64);
        assert_eq!(packet.packet_type, REQUEST_PACKET_TYPE);
        assert!(packet.body.is_empty());
    }
}
