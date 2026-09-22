//! Fixed packet-type routing for the small set of plugin behaviors the MVP
//! implements.
//!
//! This is deliberately not a generic or dynamic plugin system: adding a new
//! packet family means adding a match arm to [`dispatch_incoming`] and an
//! entry to [`capabilities`], not registering a trait object at runtime.
//! `plugins` depends only on `protocol`; it has no knowledge of HTTP,
//! transport sockets, or CLI types, so it can be exercised without a
//! connection or an application handle.

pub mod clipboard;
pub mod ping;

use thiserror::Error;

use crate::protocol::{BodyError, Packet};

/// Capability strings advertised by all packet handlers registered here.
/// These are copied verbatim into the `incomingCapabilities` and
/// `outgoingCapabilities` fields of the local identity packet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginCapabilities {
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
}

/// The fixed set of packet types this build can send and receive.
pub fn capabilities() -> PluginCapabilities {
    PluginCapabilities {
        incoming: vec![
            ping::PACKET_TYPE.to_owned(),
            clipboard::PACKET_TYPE.to_owned(),
            clipboard::CONNECT_PACKET_TYPE.to_owned(),
        ],
        outgoing: vec![
            ping::PACKET_TYPE.to_owned(),
            clipboard::PACKET_TYPE.to_owned(),
            clipboard::CONNECT_PACKET_TYPE.to_owned(),
        ],
    }
}

/// A packet successfully routed to a registered plugin handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IncomingPluginPacket {
    Ping(ping::PingBody),
    Clipboard(clipboard::ClipboardBody),
    ClipboardConnect(clipboard::ClipboardConnectBody),
}

#[derive(Debug, Error)]
pub enum PluginDispatchError {
    #[error("packet type {0:?} has no registered plugin handler")]
    Unrecognized(String),
    #[error("packet body could not be decoded")]
    InvalidBody(#[from] BodyError),
}

/// Route a decoded packet to its registered plugin handler by packet type.
///
/// Callers are responsible for enforcing that only paired devices reach
/// this function (see `ApplicationHandle::handle_peer_packet`); capability
/// filtering for outgoing packets is a separate, caller-side concern based
/// on the peer's advertised `incomingCapabilities`.
pub fn dispatch_incoming(packet: &Packet) -> Result<IncomingPluginPacket, PluginDispatchError> {
    match packet.packet_type.as_str() {
        ping::PACKET_TYPE => Ok(IncomingPluginPacket::Ping(packet.body_as()?)),
        clipboard::PACKET_TYPE => Ok(IncomingPluginPacket::Clipboard(packet.body_as()?)),
        clipboard::CONNECT_PACKET_TYPE => {
            Ok(IncomingPluginPacket::ClipboardConnect(packet.body_as()?))
        }
        other => Err(PluginDispatchError::Unrecognized(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertises_ping_and_clipboard_both_directions() {
        let capabilities = capabilities();
        let expected = vec![
            ping::PACKET_TYPE.to_owned(),
            clipboard::PACKET_TYPE.to_owned(),
            clipboard::CONNECT_PACKET_TYPE.to_owned(),
        ];
        assert_eq!(capabilities.incoming, expected);
        assert_eq!(capabilities.outgoing, expected);
    }

    #[test]
    fn unknown_packet_type_is_rejected() {
        let packet =
            Packet::from_body(1_u64, "kdeconnect.mock.echo", &serde_json::json!({})).unwrap();
        assert!(matches!(
            dispatch_incoming(&packet),
            Err(PluginDispatchError::Unrecognized(t)) if t == "kdeconnect.mock.echo"
        ));
    }

    #[test]
    fn ping_packet_dispatches_to_the_ping_handler() {
        let packet = ping::build_packet(1_u64, None).unwrap();
        assert!(matches!(
            dispatch_incoming(&packet),
            Ok(IncomingPluginPacket::Ping(_))
        ));
    }

    #[test]
    fn clipboard_packets_dispatch_to_the_clipboard_handlers() {
        let packet = clipboard::build_packet(1_u64, "hello".into()).unwrap();
        assert!(matches!(
            dispatch_incoming(&packet),
            Ok(IncomingPluginPacket::Clipboard(_))
        ));

        let connect_packet = clipboard::build_connect_packet(1_u64, "hello".into(), 1).unwrap();
        assert!(matches!(
            dispatch_incoming(&connect_packet),
            Ok(IncomingPluginPacket::ClipboardConnect(_))
        ));
    }
}
