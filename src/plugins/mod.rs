//! The daemon's features.
//!
//! A feature implements [`crate::application::Plugin`] and is listed in
//! [`builtin`]; ping is the first to do so. The others are still routed by
//! the fixed table below ([`dispatch_incoming`], [`legacy_capabilities`])
//! while they move over (see `docs/research/feature-modules.md`). The set
//! is fixed at compile time; nothing is loaded at runtime.

pub mod battery;
pub mod clipboard;
pub mod findmyphone;
pub mod ping;
pub mod sftp;
pub mod share;

use std::sync::Arc;

use thiserror::Error;

use crate::{
    application::{Plugin, PluginRegistry},
    protocol::{BodyError, Packet},
};

/// Every plugin in this build.
pub fn builtin() -> Vec<Arc<dyn Plugin>> {
    vec![Arc::new(ping::PingPlugin)]
}

/// Capability strings advertised by all packet handlers registered here.
/// These are copied verbatim into the `incomingCapabilities` and
/// `outgoingCapabilities` fields of the local identity packet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginCapabilities {
    pub incoming: Vec<String>,
    pub outgoing: Vec<String>,
}

/// The packet types this build can send and receive: those of the
/// [`builtin`] plugins, then those still in the fixed table.
pub fn capabilities() -> PluginCapabilities {
    let registry = PluginRegistry::new(builtin());
    let legacy = legacy_capabilities();
    PluginCapabilities {
        incoming: registry
            .incoming()
            .map(str::to_owned)
            .chain(legacy.incoming)
            .collect(),
        outgoing: registry
            .outgoing()
            .map(str::to_owned)
            .chain(legacy.outgoing)
            .collect(),
    }
}

/// The packet types of the features not yet moved to a plugin. Browsing,
/// battery reports and ringing are one-way: this build asks peers to serve
/// files and to ring, and reads their battery, but serves no files, doesn't
/// ring and reports no battery.
fn legacy_capabilities() -> PluginCapabilities {
    PluginCapabilities {
        incoming: vec![
            clipboard::PACKET_TYPE.to_owned(),
            clipboard::CONNECT_PACKET_TYPE.to_owned(),
            share::PACKET_TYPE.to_owned(),
            sftp::PACKET_TYPE.to_owned(),
            battery::PACKET_TYPE.to_owned(),
        ],
        outgoing: vec![
            clipboard::PACKET_TYPE.to_owned(),
            clipboard::CONNECT_PACKET_TYPE.to_owned(),
            share::PACKET_TYPE.to_owned(),
            sftp::REQUEST_PACKET_TYPE.to_owned(),
            findmyphone::REQUEST_PACKET_TYPE.to_owned(),
        ],
    }
}

/// A packet successfully routed to a registered plugin handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IncomingPluginPacket {
    Clipboard(clipboard::ClipboardBody),
    ClipboardConnect(clipboard::ClipboardConnectBody),
    ShareRequest(share::ShareRequestBody),
    ShareRequestUpdate(share::ShareRequestUpdateBody),
    Sftp(sftp::SftpBody),
    Battery(battery::BatteryBody),
}

#[derive(Debug, Error)]
pub enum PluginDispatchError {
    #[error("packet type {0:?} has no registered plugin handler")]
    Unrecognized(String),
    #[error("packet body could not be decoded")]
    InvalidBody(#[from] BodyError),
}

/// Route a packet of a feature still in the fixed table by packet type.
///
/// Callers are responsible for enforcing that only paired devices reach
/// this function (see `ApplicationHandle::handle_peer_packet`); capability
/// filtering for outgoing packets is a separate, caller-side concern based
/// on the peer's advertised `incomingCapabilities`.
pub fn dispatch_incoming(packet: &Packet) -> Result<IncomingPluginPacket, PluginDispatchError> {
    match packet.packet_type.as_str() {
        clipboard::PACKET_TYPE => Ok(IncomingPluginPacket::Clipboard(packet.body_as()?)),
        clipboard::CONNECT_PACKET_TYPE => {
            Ok(IncomingPluginPacket::ClipboardConnect(packet.body_as()?))
        }
        share::PACKET_TYPE => Ok(IncomingPluginPacket::ShareRequest(packet.body_as()?)),
        share::UPDATE_PACKET_TYPE => {
            Ok(IncomingPluginPacket::ShareRequestUpdate(packet.body_as()?))
        }
        sftp::PACKET_TYPE => Ok(IncomingPluginPacket::Sftp(packet.body_as()?)),
        battery::PACKET_TYPE => Ok(IncomingPluginPacket::Battery(packet.body_as()?)),
        other => Err(PluginDispatchError::Unrecognized(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertises_ping_clipboard_and_share_both_directions_and_the_rest_one_way() {
        let capabilities = capabilities();
        let bidirectional = [
            ping::PACKET_TYPE,
            clipboard::PACKET_TYPE,
            clipboard::CONNECT_PACKET_TYPE,
            share::PACKET_TYPE,
        ];
        assert_eq!(
            capabilities.incoming,
            [
                &bidirectional[..],
                &[sftp::PACKET_TYPE, battery::PACKET_TYPE]
            ]
            .concat()
        );
        assert_eq!(
            capabilities.outgoing,
            [
                &bidirectional[..],
                &[sftp::REQUEST_PACKET_TYPE, findmyphone::REQUEST_PACKET_TYPE]
            ]
            .concat()
        );
    }

    #[test]
    fn sftp_replies_dispatch_to_the_sftp_handler() {
        let packet = Packet::from_body(
            1_u64,
            sftp::PACKET_TYPE,
            &serde_json::json!({"serverRunning": false}),
        )
        .unwrap();
        assert!(matches!(
            dispatch_incoming(&packet),
            Ok(IncomingPluginPacket::Sftp(body)) if body.server_running == Some(false)
        ));
    }

    #[test]
    fn share_packets_dispatch_to_the_share_handlers() {
        let packet =
            share::build_request_packet(1_u64, "photo.jpg".into(), None, 10, 1741).unwrap();
        assert!(matches!(
            dispatch_incoming(&packet),
            Ok(IncomingPluginPacket::ShareRequest(_))
        ));

        let update_packet = share::build_update_packet(1_u64, 1, 10).unwrap();
        assert!(matches!(
            dispatch_incoming(&update_packet),
            Ok(IncomingPluginPacket::ShareRequestUpdate(_))
        ));
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
