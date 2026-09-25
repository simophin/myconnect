//! The daemon's features.
//!
//! A feature implements [`crate::application::Plugin`] and is listed in
//! [`builtin`]; so far ping, find my phone, battery, clipboard and share
//! do. Browsing is still routed by the fixed table below ([`dispatch_incoming`], [`legacy_capabilities`])
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

/// Every plugin in this build. `clipboard` is the clipboard that clipboard
/// sync reads and writes: the desktop's, or an in-memory one.
pub fn builtin(
    clipboard: Arc<dyn clipboard::ClipboardService + Send + Sync>,
) -> Vec<Arc<dyn Plugin>> {
    vec![
        Arc::new(ping::PingPlugin),
        Arc::new(findmyphone::FindMyPhonePlugin),
        Arc::new(battery::BatteryPlugin::default()),
        Arc::new(clipboard::ClipboardPlugin::new(clipboard)),
        Arc::new(share::SharePlugin),
    ]
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
    let registry = PluginRegistry::new(builtin(clipboard::InMemoryClipboard::shared()));
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

/// The packet types of the features not yet moved to a plugin. Browsing is
/// one-way: this build asks peers to serve files, but serves none.
fn legacy_capabilities() -> PluginCapabilities {
    PluginCapabilities {
        incoming: vec![sftp::PACKET_TYPE.to_owned()],
        outgoing: vec![sftp::REQUEST_PACKET_TYPE.to_owned()],
    }
}

/// A packet successfully routed to a registered plugin handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IncomingPluginPacket {
    Sftp(sftp::SftpBody),
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
        sftp::PACKET_TYPE => Ok(IncomingPluginPacket::Sftp(packet.body_as()?)),
        other => Err(PluginDispatchError::Unrecognized(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertises_ping_clipboard_and_share_both_directions_and_the_rest_one_way() {
        // Order doesn't matter to peers, so compare sorted lists.
        fn sorted(mut values: Vec<String>) -> Vec<String> {
            values.sort();
            values
        }
        fn strings(values: &[&str]) -> Vec<String> {
            sorted(values.iter().map(|value| value.to_string()).collect())
        }
        let capabilities = capabilities();
        let bidirectional = [
            ping::PACKET_TYPE,
            clipboard::PACKET_TYPE,
            clipboard::CONNECT_PACKET_TYPE,
            share::PACKET_TYPE,
        ];
        assert_eq!(
            sorted(capabilities.incoming),
            strings(
                &[
                    &bidirectional[..],
                    &[sftp::PACKET_TYPE, battery::PACKET_TYPE]
                ]
                .concat()
            )
        );
        assert_eq!(
            sorted(capabilities.outgoing),
            strings(
                &[
                    &bidirectional[..],
                    &[sftp::REQUEST_PACKET_TYPE, findmyphone::REQUEST_PACKET_TYPE]
                ]
                .concat()
            )
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
    fn unknown_packet_type_is_rejected() {
        let packet =
            Packet::from_body(1_u64, "kdeconnect.mock.echo", &serde_json::json!({})).unwrap();
        assert!(matches!(
            dispatch_incoming(&packet),
            Err(PluginDispatchError::Unrecognized(t)) if t == "kdeconnect.mock.echo"
        ));
    }
}
