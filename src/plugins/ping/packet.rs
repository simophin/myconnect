//! `kdeconnect.ping` packet model.
//!
//! Ping packets are notifications, not request/response messages: a peer
//! that receives one is not expected to reply. The optional `message` field
//! lets a sender attach freeform text; a plain ping omits it entirely.

use serde::{Deserialize, Serialize};
use serde_json::Number;

use crate::protocol::{BodyError, Packet};

/// The packet type and capability identifier for KDE Connect ping exchange.
pub const PACKET_TYPE: &str = "kdeconnect.ping";

/// Body of a `kdeconnect.ping` packet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PingBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Build a `kdeconnect.ping` packet, optionally carrying a text message.
pub fn build_packet(id: impl Into<Number>, message: Option<String>) -> Result<Packet, BodyError> {
    Packet::from_body(id, PACKET_TYPE, &PingBody { message })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_a_packet() {
        let packet = build_packet(1_u64, Some("hello".into())).unwrap();
        assert_eq!(packet.packet_type, PACKET_TYPE);
        let body: PingBody = packet.body_as().unwrap();
        assert_eq!(body.message.as_deref(), Some("hello"));
    }

    #[test]
    fn omits_message_field_when_absent() {
        let packet = build_packet(1_u64, None).unwrap();
        assert!(!packet.body.contains_key("message"));
        let body: PingBody = packet.body_as().unwrap();
        assert_eq!(body.message, None);
    }
}
