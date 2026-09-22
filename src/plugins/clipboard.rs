//! `kdeconnect.clipboard` and `kdeconnect.clipboard.connect` packet models.
//!
//! `kdeconnect.clipboard` is a plain notification carrying only the current
//! text content; it has no timestamp and is applied unconditionally (subject
//! to duplicate-content and feedback-loop guards enforced by the
//! application layer). `kdeconnect.clipboard.connect` additionally carries a
//! Unix millisecond timestamp and is sent once when a connection to a paired
//! peer is established, so the receiver can decide whether the peer's
//! clipboard is newer than what it already has before applying it: a stale
//! (not strictly newer) timestamp must be ignored.
//!
//! Never log the `content` field of either body.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

use crate::protocol::{BodyError, Packet};

/// Packet type and capability identifier for a plain clipboard update.
pub const PACKET_TYPE: &str = "kdeconnect.clipboard";
/// Packet type and capability identifier for the connect-time clipboard sync.
pub const CONNECT_PACKET_TYPE: &str = "kdeconnect.clipboard.connect";

/// Body of a `kdeconnect.clipboard` packet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardBody {
    pub content: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Body of a `kdeconnect.clipboard.connect` packet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardConnectBody {
    pub content: String,
    pub timestamp: i64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Build a `kdeconnect.clipboard` packet.
pub fn build_packet(id: impl Into<Number>, content: String) -> Result<Packet, BodyError> {
    Packet::from_body(
        id,
        PACKET_TYPE,
        &ClipboardBody {
            content,
            extra: Map::new(),
        },
    )
}

/// Build a `kdeconnect.clipboard.connect` packet.
pub fn build_connect_packet(
    id: impl Into<Number>,
    content: String,
    timestamp: i64,
) -> Result<Packet, BodyError> {
    Packet::from_body(
        id,
        CONNECT_PACKET_TYPE,
        &ClipboardConnectBody {
            content,
            timestamp,
            extra: Map::new(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_packet_round_trips_content() {
        let packet = build_packet(1_u64, "hello".into()).unwrap();
        assert_eq!(packet.packet_type, PACKET_TYPE);
        let body: ClipboardBody = packet.body_as().unwrap();
        assert_eq!(body.content, "hello");
    }

    #[test]
    fn connect_packet_round_trips_content_and_timestamp() {
        let packet = build_connect_packet(1_u64, "hello".into(), 1_700_000_000_000).unwrap();
        assert_eq!(packet.packet_type, CONNECT_PACKET_TYPE);
        let body: ClipboardConnectBody = packet.body_as().unwrap();
        assert_eq!(body.content, "hello");
        assert_eq!(body.timestamp, 1_700_000_000_000);
    }

    #[test]
    fn unknown_body_fields_survive_round_trip() {
        let mut packet = build_packet(1_u64, "hello".into()).unwrap();
        packet
            .body
            .insert("mimeType".into(), serde_json::json!("text/plain"));
        let body: ClipboardBody = packet.body_as().unwrap();
        assert_eq!(body.extra["mimeType"], "text/plain");
    }
}
